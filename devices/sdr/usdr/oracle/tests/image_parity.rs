//! rsl-usdr's gateware image header parser and update check match libusdr's
//! (`ipblks/xlnx_bitstream.c`): the same verdict and the same fields, those left by a
//! refused header included, for hand-made images and for thousands of generated and
//! corrupted ones, under every combination of libusdr's parse flags.

#[path = "../../tests/common/bitstream.rs"]
#[allow(
    dead_code,
    reason = "each test crate uses part of the shared image builder"
)]
mod bitstream;

use bitstream::{
    AXSS, BUS_WIDTH, BYTE_ORDER, Bitstream, CMD, CRC, CRC_SKIPPED, DUMMY, Facts, IDCODE, IPROG,
    NOOP, RCRC, SYNC, WBSTAR, WRITE, XC7A35T, XC7A50T, type1, type2,
};
use rsl_usdr::flash::{HeaderFault, ImageHeader, MASTER_IMAGE_OFFSET, ParseOptions, Slot};
use rsl_usdr_oracle::bitstream::{ALLOW_CROP, CRC_CHECK, ImageParams, iprgcheck, parse_header};

/// `-EINVAL`.
const EINVAL: i32 = -22;
/// `-ENOENT`.
const ENOENT: i32 = -2;
/// `-EBADMSG`.
const EBADMSG: i32 = -74;

/// Every combination of libusdr's parse flags.
const FLAGS: [u32; 4] = [0, CRC_CHECK, ALLOW_CROP, CRC_CHECK | ALLOW_CROP];

/// rsl-usdr's options for libusdr's `flags`.
fn options(flags: u32) -> ParseOptions {
    let options = ParseOptions::new();
    let options = if flags & CRC_CHECK == 0 {
        options
    } else {
        options.check_crc()
    };
    if flags & ALLOW_CROP == 0 {
        options
    } else {
        options.allow_crop()
    }
}

/// rsl-usdr's verdict as libusdr reports one: its return value and the fields it left.
fn rsl_usdr(bytes: &[u8], flags: u32) -> (i32, ImageParams) {
    let (errno, header) = match ImageHeader::parse(bytes, options(flags)) {
        Ok(header) => (0, header),
        Err(error) => (errno(error.fault()), error.partial()),
    };
    (errno, params(&header))
}

/// The errno libusdr returns for `fault`.
fn errno(fault: HeaderFault) -> i32 {
    match fault {
        HeaderFault::CrcMismatch { .. } | HeaderFault::NoCrc => EBADMSG,
        HeaderFault::MissingFields { .. } => ENOENT,
        _ => EINVAL,
    }
}

/// `header` as libusdr's struct.
fn params(header: &ImageHeader) -> ImageParams {
    ImageParams {
        devid: header.device_id,
        wbstar: header.warm_boot_start,
        usr_access2: header.build,
        iprog: header.reprograms,
    }
}

/// Requires both parsers to agree on `bytes` under every flag combination, and returns
/// libusdr's verdicts.
fn assert_parsed_alike(bytes: &[u8], case: &str) -> [(i32, ImageParams); 4] {
    FLAGS.map(|flags| {
        let c = parse_header(bytes, flags);
        assert_eq!(
            rsl_usdr(bytes, flags),
            c,
            "{case}, flags {flags}: verdicts differ"
        );
        c
    })
}

#[test]
fn golden_and_master_images_parse_like_libusdr() {
    for (name, facts) in [
        ("golden", Facts::golden(0x1234_5678)),
        ("master", Facts::master(0x8765_4321)),
    ] {
        let bytes = facts.bytes();
        let verdicts = assert_parsed_alike(&bytes, name);
        let expected = ImageParams {
            devid: XC7A35T,
            wbstar: facts.warm_boot_start,
            usr_access2: facts.build,
            iprog: facts.reprograms,
        };
        assert_eq!(
            verdicts.map(|(errno, _)| errno),
            [0; 4],
            "{name}: libusdr accepts it"
        );
        assert_eq!(verdicts[1].1, expected, "{name}: libusdr reads its facts");
    }
}

#[test]
fn every_prefix_of_an_image_parses_like_libusdr() {
    // Within the header, then into and past the frame data, at every byte alignment.
    let bytes = Facts {
        frames: 40,
        ..Facts::golden(7)
    }
    .bytes();
    for len in 0..=bytes.len() {
        assert_parsed_alike(&bytes[..len], &format!("prefix {len}"));
    }
}

/// A hand-made image: its name, bytes and libusdr's verdicts under flags 0, CRC, crop, both.
type Case = (&'static str, Vec<u8>, [i32; 4]);

/// Requires both parsers to agree on each case, and libusdr to give the verdicts written
/// down for it.
fn assert_cases(cases: Vec<Case>) {
    for (name, bytes, expected) in cases {
        let verdicts = assert_parsed_alike(&bytes, name);
        assert_eq!(
            verdicts.map(|(errno, _)| errno),
            expected,
            "{name}: libusdr's verdicts under flags 0, CRC, crop, both"
        );
    }
}

/// A bitstream with the two writes an accepted header needs.
fn header(b: Bitstream) -> Bitstream {
    b.write(IDCODE, &[XC7A35T]).write(WBSTAR, &[0])
}

#[test]
fn hand_made_preambles_parse_like_libusdr() {
    assert_cases(vec![
        ("empty", vec![], [EINVAL; 4]),
        (
            "padding only",
            Bitstream::default().raw(&[DUMMY; 4]).bytes(),
            [EINVAL; 4],
        ),
        (
            "no sync word",
            Bitstream::default()
                .raw(&[DUMMY, BYTE_ORDER, BUS_WIDTH, DUMMY])
                .bytes(),
            [ENOENT, EBADMSG, ENOENT, EBADMSG],
        ),
        (
            "no byte order word",
            Bitstream::default().raw(&[DUMMY, BUS_WIDTH, SYNC]).bytes(),
            [EINVAL; 4],
        ),
        (
            "garbage before the sync word",
            Bitstream::default()
                .raw(&[BYTE_ORDER, BUS_WIDTH, 0x1234, SYNC])
                .bytes(),
            [EINVAL; 4],
        ),
    ]);
}

#[test]
fn hand_made_crcs_parse_like_libusdr() {
    assert_cases(vec![
        (
            "no CRC word",
            header(Bitstream::new()).bytes(),
            [0, EBADMSG, 0, EBADMSG],
        ),
        (
            "with its CRC word",
            header(Bitstream::new()).crc().bytes(),
            [0; 4],
        ),
        (
            "a wrong CRC word",
            header(Bitstream::new()).bad_crc().bytes(),
            [0, EBADMSG, 0, EBADMSG],
        ),
        (
            "a CRC reset before the CRC word",
            header(Bitstream::new())
                .write(CMD, &[RCRC])
                .write(AXSS, &[5])
                .crc()
                .bytes(),
            [0; 4],
        ),
        (
            "writes to every register libusdr leaves out of its CRC",
            CRC_SKIPPED
                .iter()
                .fold(header(Bitstream::new()), |b, &r| b.write(r, &[0xdead_beef]))
                .crc()
                .bytes(),
            [0; 4],
        ),
    ]);
}

#[test]
fn hand_made_packets_parse_like_libusdr() {
    assert_cases(vec![
        (
            "no IDCODE",
            Bitstream::new().write(WBSTAR, &[0]).crc().bytes(),
            [ENOENT; 4],
        ),
        (
            "no WBSTAR",
            Bitstream::new().write(IDCODE, &[XC7A35T]).crc().bytes(),
            [ENOENT; 4],
        ),
        (
            "a cut-off packet",
            header(Bitstream::new())
                .raw(&[type1(WRITE, AXSS, 3), 9])
                .bytes(),
            [EINVAL, EINVAL, 0, EBADMSG],
        ),
        (
            "a cut-off type 2 packet",
            header(Bitstream::new())
                .write(AXSS, &[])
                .raw(&[type2(WRITE, 0x7ff_ffff), 1, 2])
                .bytes(),
            [EINVAL, EINVAL, 0, EBADMSG],
        ),
        (
            "a type 3 packet",
            header(Bitstream::new()).raw(&[0x6000_0000]).bytes(),
            [EINVAL; 4],
        ),
        (
            "a type 2 packet with no type 1 before it",
            Bitstream::new()
                .raw(&[type2(WRITE, 1), 0x77])
                .write(IDCODE, &[1])
                .write(WBSTAR, &[2])
                .bytes(),
            [0, EBADMSG, 0, EBADMSG],
        ),
        (
            "a read packet whose words are read as headers",
            header(Bitstream::new())
                .raw(&[type1(1, AXSS, 1), NOOP])
                .bytes(),
            [0, EBADMSG, 0, EBADMSG],
        ),
        (
            "IPROG then another command",
            header(Bitstream::new())
                .write(CMD, &[IPROG])
                .write(CMD, &[0x0d])
                .crc()
                .bytes(),
            [0; 4],
        ),
        (
            "a register number past five bits",
            header(Bitstream::new())
                .write(IDCODE | 0x100, &[0x55])
                .crc()
                .bytes(),
            [0; 4],
        ),
        (
            "a trailing partial word",
            [header(Bitstream::new()).crc().bytes(), vec![0xaa, 0x99]].concat(),
            [0; 4],
        ),
        (
            "a CRC word at the very end, cut off",
            header(Bitstream::new())
                .raw(&[type1(WRITE, CRC, 1)])
                .bytes(),
            [EINVAL, EINVAL, 0, EBADMSG],
        ),
    ]);
}

/// A seeded xorshift64 generator: deterministic cases without a dependency.
struct Rng(u64);

impl Rng {
    /// The next 64 random bits.
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A number below `n`.
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(n).expect("small")).expect("below n")
    }

    /// 32 random bits.
    fn word(&mut self) -> u32 {
        u32::try_from(self.next() >> 32).expect("32 bits")
    }

    /// One of `items`.
    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())]
    }
}

/// A random image built from what libusdr's parser distinguishes: preamble words, writes
/// to the registers it reads or skips, type 2 packets, CRC resets, right and wrong CRC
/// words, non-write packets and stray words.
fn generated(rng: &mut Rng) -> Vec<u8> {
    let registers = [
        CRC, CMD, IDCODE, AXSS, WBSTAR, 0x0f, 0x12, 0x14, 0x15, 0x16, 0x02, 0x1f,
    ];
    let mut b = if rng.below(8) == 0 {
        let preamble = [DUMMY, BYTE_ORDER, BUS_WIDTH, SYNC, NOOP, rng.word()];
        let words: Vec<u32> = (0..rng.below(6)).map(|_| rng.pick(&preamble)).collect();
        Bitstream::default().raw(&words)
    } else {
        Bitstream::new()
    };
    // Mostly with the two writes an accepted header needs, so both verdicts are common.
    if rng.below(4) != 0 {
        b = b.write(IDCODE, &[rng.word()]).write(WBSTAR, &[rng.word()]);
    }
    for _ in 0..rng.below(24) {
        b = match rng.below(12) {
            0 => b.crc(),
            1 => b.bad_crc(),
            2 => {
                let commands = [RCRC, IPROG, 0, rng.word()];
                b.write(CMD, &[rng.pick(&commands)])
            }
            3 => b.raw(&[NOOP]),
            4 => b.raw(&[rng.word()]),
            5 => {
                let values: Vec<u32> = (0..rng.below(5)).map(|_| rng.word()).collect();
                b.long_write(rng.pick(&registers), &values)
            }
            6 => b.raw(&[
                type1(rng.pick(&[0, 1, 3]), rng.pick(&registers), 1),
                rng.word(),
            ]),
            _ => {
                let values: Vec<u32> = (0..=rng.below(3)).map(|_| rng.word()).collect();
                b.write(rng.pick(&registers), &values)
            }
        };
    }
    // Often closed by a right CRC word, so the CRC check accepts some.
    if rng.below(2) == 0 {
        b = b.crc();
    }
    let mut bytes = b.bytes();
    // Then corruptions: flipped bits, overwritten, dropped or cut-off bytes.
    for _ in 0..rng.below(3) {
        if bytes.is_empty() {
            break;
        }
        let at = rng.below(bytes.len());
        match rng.below(4) {
            0 => bytes[at] ^= 1 << rng.below(8),
            1 => bytes[at] = u8::try_from(rng.below(256)).expect("a byte"),
            2 => drop(bytes.remove(at)),
            _ => bytes.truncate(at),
        }
    }
    bytes
}

#[test]
fn generated_and_corrupted_images_parse_like_libusdr() {
    let mut rng = Rng(0x5eed_f1a5_4b17_0001);
    let mut accepted = [0_usize; 4];
    let cases = 20_000;
    for case in 0..cases {
        let bytes = generated(&mut rng);
        let verdicts = assert_parsed_alike(&bytes, &format!("case {case}: {bytes:02x?}"));
        for (count, (errno, _)) in accepted.iter_mut().zip(verdicts) {
            *count += usize::from(errno == 0);
        }
    }
    // The cases must exercise both verdicts under every flag combination.
    for (flags, count) in FLAGS.iter().zip(accepted) {
        assert!(
            (cases / 50..cases - cases / 50).contains(&count),
            "flags {flags}: {count} of {cases} accepted, too one-sided to compare"
        );
    }
}

#[test]
fn slot_checks_match_libusdrs_for_every_combination() {
    let headers: Vec<ImageHeader> = [XC7A35T, XC7A50T]
        .into_iter()
        .flat_map(|device_id| {
            [0, MASTER_IMAGE_OFFSET, 0x30_0000]
                .into_iter()
                .flat_map(move |warm_boot_start| {
                    [false, true].map(|reprograms| ImageHeader {
                        device_id,
                        warm_boot_start,
                        build: 1,
                        reprograms,
                    })
                })
        })
        .collect();
    let mut admitted = 0;
    for golden in &headers {
        for image in &headers {
            for (slot, to_golden) in [(Slot::Golden, true), (Slot::Master, false)] {
                let c = iprgcheck(
                    &params(golden),
                    &params(image),
                    MASTER_IMAGE_OFFSET,
                    to_golden,
                );
                let rust = image.check_for(slot, golden);
                assert_eq!(
                    rust.map_or(EINVAL, |()| 0),
                    c,
                    "{image:?} into the {slot} slot beside {golden:?}: {rust:?}"
                );
                admitted += usize::from(c == 0);
            }
        }
    }
    assert!(admitted > 0, "some combination is admitted");
}
