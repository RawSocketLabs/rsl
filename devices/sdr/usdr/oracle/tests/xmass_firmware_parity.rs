//! usdr's XMASS switch firmware update matches libusdr's `tools/xmass_fwupd.c`: the
//! same requests to the XMASS device, in the same order, the same verdict and the same
//! flash afterwards, on one fake `/dev/xmassN` (`usdr_oracle::xmass`), for the update,
//! `info`, a failure at each request and a read-back that differs.
//!
//! The request codes themselves are checked against the driver header by `pcie_abi.rs`;
//! the fake decodes the tool's with the header's, so a code the tool got wrong shows here
//! as a request the driver does not know. Two divergences are whitelisted:
//! `a_long_file_is_cut_by_libusdr_and_refused_by_usdr` and
//! `a_read_returning_a_failure_code_passes_libusdr_and_fails_usdr`.

use std::fs;
use std::io;
use std::path::PathBuf;

use usdr::frontend::xmass_firmware::{
    self, FIRMWARE_BYTES, Firmware, FirmwareSizeError, IDS_BYTES, READ_SUCCESS, SwitchFlash,
    SwitchIds, UnexpectedReturn, UpdateError,
};
use usdr_oracle::xmass::{FLASH_BYTES, FakeXmass, Request, XmassRun, xmass_fwupd};

/// Where both drivers find the device.
const NODE: &str = "/dev/xmass0";

/// `O_RDWR`, how the tool opens it.
const O_RDWR: i32 = 2;

/// `ETIMEDOUT`, the driver's answer when the switch's SPI master does not respond.
const ETIMEDOUT: i32 = 110;

/// The tool's `EXIT_FAILURE`.
const FAILED: i32 = 1;

/// The IDs the fake reports: firmware `221012000001`, then a JEDEC ID reply `c2 20 11` and
/// a fourth byte.
const IDS: [u8; IDS_BYTES] = [0x22, 0x10, 0x12, 0x00, 0x00, 0x01, 0xc2, 0x20, 0x11, 0xc2];

/// A switch reporting [`IDS`] whose flash holds an older image.
fn device() -> FakeXmass {
    FakeXmass::new(IDS, &pattern(3))
}

/// 64 KiB of a pattern seeded by `seed`.
fn pattern(seed: usize) -> Box<[u8; FLASH_BYTES]> {
    let bytes: Vec<u8> = (0..FLASH_BYTES)
        .map(|i| u8::try_from((i * 31 + seed * 7) % 251).expect("below 251"))
        .collect();
    bytes.into_boxed_slice().try_into().expect("64 KiB")
}

/// Writes `bytes` to a file named for the test and returns its path.
fn file(name: &str, bytes: &[u8]) -> String {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("xmass_{name}.bin"));
    fs::write(&path, bytes).expect("writes the image file");
    path.to_str().expect("UTF-8 path").to_owned()
}

/// usdr's view of the fake: the four requests, with the driver's errno or return value.
struct Switch<'a>(&'a mut FakeXmass);

impl SwitchFlash for Switch<'_> {
    fn ids(&mut self, out: &mut [u8; IDS_BYTES]) -> io::Result<i32> {
        self.0.read_ids(out).map_err(io::Error::from_raw_os_error)
    }

    fn erase(&mut self) -> io::Result<i32> {
        self.0.erase().map_err(io::Error::from_raw_os_error)
    }

    fn write(&mut self, image: &[u8; FIRMWARE_BYTES]) -> io::Result<i32> {
        self.0.write(image).map_err(io::Error::from_raw_os_error)
    }

    fn read(&mut self, out: &mut [u8; FIRMWARE_BYTES]) -> io::Result<i32> {
        self.0.read(out).map_err(io::Error::from_raw_os_error)
    }
}

/// Runs both drivers' updates with `image` on fresh copies of `device`.
fn both(
    name: &str,
    device: &FakeXmass,
    image: &[u8],
) -> (XmassRun, Result<SwitchIds, UpdateError>, FakeXmass) {
    let tool = xmass_fwupd(device.clone(), NODE, &[&file(name, image), NODE]);
    let mut ours = device.clone();
    let firmware = Firmware::new(image).expect("a 64 KiB image");
    let result = xmass_firmware::update(&mut Switch(&mut ours), &firmware);
    (tool, result, ours)
}

/// The same requests and the same flash on both sides, and the tool opened the node
/// read-write and, unless `left_open`, closed it.
fn assert_same_device(tool: &XmassRun, ours: &FakeXmass, left_open: bool) {
    assert_eq!(
        tool.device.requests(),
        ours.requests(),
        "the same requests in the same order"
    );
    assert!(
        tool.device.flash() == ours.flash(),
        "the same flash afterwards"
    );
    assert_eq!(
        tool.opens,
        [(NODE.to_owned(), O_RDWR)],
        "one read-write open"
    );
    assert_eq!(
        tool.closes,
        usize::from(!left_open),
        "closed unless left open"
    );
}

#[test]
fn an_update_makes_libusdrs_requests_in_its_order() {
    let image = pattern(9);
    let (tool, result, ours) = both("update", &device(), &image[..]);
    assert_eq!(tool.status, 0, "the tool succeeds:\n{}", tool.log);
    let ids = result.expect("usdr succeeds");
    assert_eq!(
        (ids.firmware.as_slice(), ids.flash_id.as_slice()),
        IDS.split_at(6),
        "the IDs read before the update"
    );
    assert_same_device(&tool, &ours, false);
    assert_eq!(
        ours.requests(),
        [
            Request::Ids,
            Request::Erase,
            Request::Write(image.clone()),
            Request::Read
        ],
        "the IDs, an erase, the image and a read-back"
    );
    let outside = |flash: &[u8; FLASH_BYTES]| {
        let mut bytes = flash.to_vec();
        bytes.drain(xmass_firmware::SIGNATURE);
        bytes
    };
    assert_eq!(
        outside(ours.flash()),
        outside(&image),
        "the flash holds the image outside the range the tool does not compare"
    );
}

#[test]
fn info_prints_the_ids_usdr_reads() {
    let tool = xmass_fwupd(device(), NODE, &["info", NODE]);
    assert_eq!(tool.status, 0, "the tool succeeds:\n{}", tool.log);
    let mut ours = device();
    let ids = xmass_firmware::read_ids(&mut Switch(&mut ours)).expect("usdr reads them");
    assert_same_device(&tool, &ours, false);
    assert_eq!(ours.requests(), [Request::Ids], "the IDs alone");
    let [f0, f1, f2, f3, f4, f5] = ids.firmware;
    let firmware = format!("{f0:02x}{f1:02x}{f2:02x}{f3:02x}{f4:02x}{f5:02x}");
    let [m, t, c, _] = ids.flash_id;
    let line = format!("ASM2806 Firmware in use: {firmware}, FlashID: {m:02x} {t:02x} {c:02x}\n");
    assert!(
        tool.log.contains(&line),
        "the tool prints the firmware ID and the flash ID's first three bytes:\n{}",
        tool.log
    );
}

#[test]
fn a_failed_request_stops_both_where_libusdr_stops() {
    let image = pattern(9);
    for index in 0..4 {
        let device = device().failing(index, ETIMEDOUT);
        let (tool, result, ours) = both(&format!("failure_{index}"), &device, &image[..]);
        assert_eq!(tool.status, FAILED, "the tool fails at request {index}");
        // The tool closes the device after a failed request.
        assert_same_device(&tool, &ours, false);
        assert_eq!(
            ours.requests().len(),
            index + 1,
            "nothing after the failure"
        );
        let error = result.expect_err("usdr fails");
        let ((0, UpdateError::Ids(source))
        | (1, UpdateError::Erase(source))
        | (2, UpdateError::Write(source))
        | (3, UpdateError::ReadBack(source))) = (index, &error)
        else {
            panic!("request {index} is the step that failed, not {error:?}");
        };
        assert_eq!(source.raw_os_error(), Some(ETIMEDOUT), "the driver's errno");
    }
}

#[test]
fn a_byte_that_reads_back_wrong_fails_both_unless_the_driver_signs_there() {
    let image = pattern(9);
    for at in [0, 0xdff, 0xe00, 0xeff, 0xf00, FLASH_BYTES - 1] {
        let signed = xmass_firmware::SIGNATURE.contains(&at);
        let device = device().misreading(at);
        let (tool, result, ours) = both(&format!("misread_{at:x}"), &device, &image[..]);
        // The tool returns at a failed comparison without closing the device.
        assert_same_device(&tool, &ours, !signed);
        if signed {
            assert_eq!(tool.status, 0, "the tool skips byte {at:#x}");
            result.expect("usdr skips it too");
        } else {
            assert_eq!(tool.status, FAILED, "the tool compares byte {at:#x}");
            assert!(
                matches!(result, Err(UpdateError::VerifyFailed { first, bytes: 1, .. }) if first == at),
                "usdr names byte {at:#x}: {result:?}"
            );
        }
    }
}

#[test]
fn a_short_file_is_refused_by_both_before_the_device_is_opened() {
    let image = vec![0x5a; FLASH_BYTES - 1];
    let tool = xmass_fwupd(device(), NODE, &[&file("short", &image), NODE]);
    assert_eq!(tool.status, FAILED, "the tool refuses it");
    assert!(tool.opens.is_empty(), "the tool never opens the device");
    assert!(tool.device.requests().is_empty(), "no request");
    assert_eq!(
        Firmware::new(&image),
        Err(FirmwareSizeError {
            len: FLASH_BYTES - 1
        }),
        "usdr refuses it before any device exists"
    );
}

/// Whitelisted divergence: the tool reads the first 64 KiB of a longer file and flashes
/// them; usdr refuses the file, as it cannot tell a padded image from a wrong one.
#[test]
fn a_long_file_is_cut_by_libusdr_and_refused_by_usdr() {
    let image = pattern(9);
    let mut long = image.to_vec();
    long.push(0);
    let tool = xmass_fwupd(device(), NODE, &[&file("long", &long), NODE]);
    assert_eq!(
        tool.status, 0,
        "the tool flashes its first 64 KiB:\n{}",
        tool.log
    );
    assert_eq!(
        tool.device.requests(),
        [
            Request::Ids,
            Request::Erase,
            Request::Write(image),
            Request::Read
        ],
        "the tool's update, with the file's first 64 KiB"
    );
    assert_eq!(
        Firmware::new(&long),
        Err(FirmwareSizeError {
            len: FLASH_BYTES + 1
        }),
        "usdr refuses it before any device exists"
    );
}

/// Whitelisted divergence: the tool takes any return value that is not an error as success,
/// and the driver is observed to pass positive failure codes through. A read-back that
/// returns one hands back the image just written (here, as from a stale staging buffer),
/// so the tool verifies the image against itself; usdr accepts only the read's
/// success value.
#[test]
fn a_read_returning_a_failure_code_passes_libusdr_and_fails_usdr() {
    const PCI_CONFIG_FAILURE: i32 = 0x87;
    let image = pattern(9);
    let device = device().returning(3, PCI_CONFIG_FAILURE);
    let (tool, result, ours) = both("read_returns_code", &device, &image[..]);
    assert_eq!(tool.status, 0, "the tool reports success:\n{}", tool.log);
    assert_same_device(&tool, &ours, false);
    let error = result.expect_err("usdr refuses the read-back");
    let UpdateError::ReadBack(source) = &error else {
        panic!("the read-back failed, not {error:?}");
    };
    assert_eq!(
        source
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<UnexpectedReturn>()),
        Some(&UnexpectedReturn {
            returned: PCI_CONFIG_FAILURE,
            success: READ_SUCCESS
        }),
        "usdr names the value the driver returned"
    );
}
