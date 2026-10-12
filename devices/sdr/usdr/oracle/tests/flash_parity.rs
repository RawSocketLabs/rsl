//! rsl-usdr's flash maintenance matches libusdr's `ipblks/espi_flash.c` on the simulated
//! flash controller: the same register operations and waits, in the same order, the same
//! result and the same flash contents afterwards.
//!
//! Intentional divergences are whitelisted per test, each asserting both sides' behaviour:
//! `a_range_past_16_mib_is_refused_where_libusdr_sends_its_address`,
//! `a_misaligned_erase_remainder_is_refused_before_libusdr_would_start_erasing`,
//! `a_chunk_crossing_a_page_is_refused_where_libusdr_corrupts_the_page`,
//! `capacities_past_libusdrs_32_bit_arithmetic_are_reported_whole`.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::sync::{Arc, PoisonError};

use rsl_usdr::flash::{Flash, FlashError, FlashId, MASTER_IMAGE_OFFSET, WriteOptions};
use rsl_usdr_oracle::Oracle;
use rsl_usdr_oracle::flash::{DONT_ERASE, DONT_WRITE_HEADER, FlashOracle};
use rsl_usdr_sim::{BoardRevision, Op, SimBoard, SimFlash};
use sim_bus::SimBus;

/// The controller's register base libusdr's flash callers pass: `M2PCI_REG_QSPI_FLASH`,
/// checked against the board description by
/// `the_controller_and_master_image_are_where_libusdr_describes_them`.
const BASE: u32 = 10;
/// The transfer buffer base every libusdr caller passes (`controller_ugen.c`:42, 99); it is
/// not in the board description.
const MMAP_BASE: u32 = 512;

/// `-EINVAL`.
const EINVAL: i32 = -22;
/// `-ENODATA`: libusdr's "unknown part" alongside a valid ID.
const ENODATA: i32 = -61;
/// `-ETIMEDOUT`.
const ETIMEDOUT: i32 = -110;

/// A Micron MT25QL032 (32 Mbit, 3.3 V) ID: manufacturer 0x20, type 0xba, capacity 0x16.
const ID: u32 = 0x0016_ba20;
/// Its size, 4 MiB: big enough to hold the master image offset.
const SIZE: usize = 4 << 20;

/// Flash contents no erase or program produces, so both show: each byte a function of its
/// address.
fn pattern() -> Vec<u8> {
    (0..SIZE)
        .map(|i| u8::try_from((i ^ i >> 8 ^ i >> 16) & 0xff).expect("masked"))
        .collect()
}

/// A board with [`ID`]'s flash holding [`pattern`].
fn board() -> SimBoard {
    board_with(SimFlash::new(ID, SIZE).with_contents(0, &pattern()))
}

/// A board with `flash`.
fn board_with(flash: SimFlash) -> SimBoard {
    SimBoard::new(BoardRevision::Rev3).with_flash(flash)
}

/// Data to write: distinct from [`pattern`], with set and clear bits in every byte.
fn image(len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| u8::try_from((i * 37 + 11) % 251).expect("below 251"))
        .collect()
}

/// Runs libusdr's flash driver on `board`.
fn libusdr<T>(board: SimBoard, op: impl FnOnce(&mut FlashOracle) -> T) -> (T, SimBoard) {
    let mut oracle = FlashOracle::new(board, BASE, MMAP_BASE);
    let result = op(&mut oracle);
    (result, oracle.close())
}

/// Runs rsl-usdr's flash maintenance on `board`.
fn rsl_usdr<T>(board: SimBoard, op: impl FnOnce(&mut Flash<SimBus>) -> T) -> (T, SimBoard) {
    let bus = SimBus::new(board);
    let mut flash = Flash::new(bus.clone());
    let result = op(&mut flash);
    drop(flash);
    let board = Arc::into_inner(bus.0)
        .expect("the flash handle released its bus")
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner);
    (result, board)
}

/// The first operation at which the traces differ, if they do.
fn first_difference(libusdr: &[Op], rust: &[Op]) -> Option<usize> {
    libusdr
        .iter()
        .zip(rust)
        .position(|(c, r)| c != r)
        .or_else(|| (libusdr.len() != rust.len()).then(|| libusdr.len().min(rust.len())))
}

/// Requires both drivers to have done the same register operations and waits and left the
/// same flash contents.
fn assert_same_effect(libusdr: &SimBoard, rust: &SimBoard) {
    let (c, r) = (libusdr.trace(), rust.trace());
    if let Some(at) = first_difference(c, r) {
        panic!(
            "operation {at} differs: libusdr {:?}, rsl-usdr {:?} ({} and {} operations)",
            c.get(at),
            r.get(at),
            c.len(),
            r.len()
        );
    }
    assert!(
        contents(libusdr) == contents(rust),
        "the drivers left different flash contents"
    );
}

/// The board's flash contents.
fn contents(board: &SimBoard) -> &[u8] {
    board.flash().expect("the board has a flash").contents()
}

#[test]
fn the_controller_and_master_image_are_where_libusdr_describes_them() {
    let mut oracle = Oracle::open(SimBoard::new(BoardRevision::Rev3)).expect("opens");
    let base = oracle.get_uint(c"/ll/qspi_flash/base").expect("described");
    let master = oracle
        .get_uint(c"/ll/qspi_flash/master_off")
        .expect("described");
    let _ = oracle.close();
    assert_eq!(base, u64::from(BASE));
    assert_eq!(master, u64::from(MASTER_IMAGE_OFFSET));
}

/// Both drivers' ID reads of a flash answering `id`: libusdr's reply, and rsl-usdr's ID.
fn read_ids(id: u32) -> (rsl_usdr_oracle::flash::IdReply, FlashId) {
    let flash = || board_with(SimFlash::new(id, 1 << 16));
    let (reply, c) = libusdr(flash(), FlashOracle::get_id);
    let (rust, r) = rsl_usdr(flash(), |flash| flash.id().expect("reads"));
    assert_same_effect(&c, &r);
    (reply, rust)
}

#[test]
fn ids_and_descriptions_match_libusdr() {
    let parts = [
        0x0018_ba20, // Micron MT25QL128, 3.3 V
        0x0019_bb20, // Micron MT25QU256, 1.8 V
        0x0018_20c2, // Macronix MX25L128
        0x0016_421f, // Adesto SPI/QPI
        0x0018_40ef, // Winbond: unknown to libusdr
        0x0018_bc20, // Micron, a type libusdr does not know
        0x1018_ba20, // a fourth byte libusdr ignores
    ];
    let micron_sizes = (0..=28).map(|code| 0x0000_ba20 | code << 16);
    for id in parts.into_iter().chain(micron_sizes) {
        let (reply, rust) = read_ids(id);
        assert_eq!(reply.id, rust.raw(), "{id:#x}: ID word");
        assert_eq!(reply.description, rust.to_string(), "{id:#x}: description");
        let expected = if rust.is_recognized() { 0 } else { ENODATA };
        assert_eq!(reply.errno, expected, "{id:#x}: recognised as libusdr does");
    }
}

/// Divergence: libusdr multiplies the size by 8 in 32 bits, which wraps from capacity code
/// 29 (512 MiB) and is undefined from 32 (rsl-usdr does not run it there).
#[test]
fn capacities_past_libusdrs_32_bit_arithmetic_are_reported_whole() {
    for (code, megabits) in [(29, 4096), (30, 8192), (31, 16384)] {
        let (reply, rust) = read_ids(0x0000_ba20 | code << 16);
        assert_eq!(
            reply.description, "Micron Serial NOR MT25Q 0 Mb (3.3V)",
            "libusdr's wrapped size"
        );
        assert_eq!(
            rust.to_string(),
            format!("Micron Serial NOR MT25Q {megabits} Mb (3.3V)")
        );
    }
}

#[test]
fn reads_of_every_length_and_alignment_match_libusdr() {
    let master = MASTER_IMAGE_OFFSET;
    let end = u32::try_from(SIZE).expect("4 MiB");
    for (offset, len) in [
        (0, 0),
        (0, 1),
        (1, 3),
        (5, 4),
        (0x7f, 130),
        (0x100, 128),
        (3, 1000),
        (master, 256),
        (master + 0x1001, 517),
        (end - 16, 16),
    ] {
        let (c_result, c) = libusdr(board(), |flash| flash.read(offset, len));
        let (r_result, r) = rsl_usdr(board(), |flash| {
            let mut out = vec![0; usize::try_from(len).expect("small")];
            flash.read(offset, &mut out).map(|()| out)
        });
        let c_bytes = c_result.expect("libusdr reads");
        let r_bytes = r_result.expect("rsl-usdr reads");
        assert_eq!(c_bytes, r_bytes, "{offset:#x}+{len}: bytes read");
        let start = usize::try_from(offset).expect("small");
        assert_eq!(
            r_bytes,
            pattern()[start..start + r_bytes.len()],
            "{offset:#x}+{len}: the flash's contents"
        );
        assert_same_effect(&c, &r);
    }
}

/// Divergence: libusdr sends any 32-bit address; through the 24-bit core
/// (`USDR_QSPI_FLASH_24_RW`) one at 16 MiB or past would reach the start of the flash, the
/// golden image. rsl-usdr refuses the range before sending anything.
#[test]
fn a_range_past_16_mib_is_refused_where_libusdr_sends_its_address() {
    let offset = 1 << 24;
    let (c_result, c) = libusdr(board(), |flash| flash.read(offset, 4));
    assert!(c_result.is_ok(), "libusdr reads");
    assert!(
        c.trace().contains(&Op::RegWrite {
            addr: BASE + 1,
            value: offset
        }),
        "libusdr sent the address"
    );

    let (r_result, r) = rsl_usdr(board(), |flash| flash.read(offset, &mut [0; 4]));
    assert!(
        matches!(r_result, Err(FlashError::OutOfRange { .. })),
        "{r_result:?}"
    );
    assert!(r.trace().is_empty(), "nothing sent");
}

#[test]
fn erases_match_libusdrs_choice_of_sectors_and_subsectors() {
    for (offset, len) in [
        (0x1000, 0x1000),
        (0x1_0000, 0x1_0000),
        (0xf000, 0x2_2000),
        (MASTER_IMAGE_OFFSET, 0x3_0000),
    ] {
        let (c_result, c) = libusdr(board(), |flash| flash.erase(offset, len));
        let (r_result, r) = rsl_usdr(board(), |flash| flash.programmer().erase(offset, len));
        assert_eq!(c_result, Ok(()), "{offset:#x}+{len:#x}: libusdr erases");
        assert!(r_result.is_ok(), "{offset:#x}+{len:#x}: {r_result:?}");
        assert_same_effect(&c, &r);
        let range =
            usize::try_from(offset).expect("small")..usize::try_from(offset + len).expect("small");
        assert!(
            contents(&r)[range].iter().all(|&byte| byte == 0xff),
            "erased"
        );
    }
}

#[test]
fn a_chip_erase_matches_libusdrs_whole_flash_erase() {
    let (c_result, c) = libusdr(board(), |flash| flash.erase(0, u32::MAX));
    let (r_result, r) = rsl_usdr(board(), |flash| flash.programmer().erase_chip());
    assert_eq!(c_result, Ok(()));
    assert!(r_result.is_ok(), "{r_result:?}");
    assert_same_effect(&c, &r);
    assert!(contents(&r).iter().all(|&byte| byte == 0xff));
}

#[test]
fn erases_both_drivers_refuse_send_nothing() {
    for (offset, len) in [(0, 0), (0, 0xfff), (0x800, 0x2000)] {
        let (c_result, c) = libusdr(board(), |flash| flash.erase(offset, len));
        let (r_result, r) = rsl_usdr(board(), |flash| flash.programmer().erase(offset, len));
        assert_eq!(c_result, Err(EINVAL), "{offset:#x}+{len:#x}");
        assert!(
            matches!(r_result, Err(FlashError::EraseGranularity { .. })),
            "{offset:#x}+{len:#x}: {r_result:?}"
        );
        assert!(c.trace().is_empty() && r.trace().is_empty());
    }
}

/// Divergence: libusdr erases block by block and fails only at a remainder smaller than a
/// subsector, after erasing what came before; rsl-usdr refuses the whole range up front.
#[test]
fn a_misaligned_erase_remainder_is_refused_before_libusdr_would_start_erasing() {
    let (offset, len) = (0, 0x1800);
    let (c_result, c) = libusdr(board(), |flash| flash.erase(offset, len));
    assert_eq!(c_result, Err(EINVAL));
    assert!(
        contents(&c)[..0x1000].iter().all(|&byte| byte == 0xff),
        "libusdr erased the first subsector before failing"
    );

    let (r_result, r) = rsl_usdr(board(), |flash| flash.programmer().erase(offset, len));
    assert!(matches!(r_result, Err(FlashError::EraseGranularity { .. })));
    assert!(r.trace().is_empty(), "nothing sent");

    // The same range through a write that erases first.
    let data = image(0x1800);
    let (c_result, c) = libusdr(board(), |flash| flash.write(offset, &data, 0));
    assert_eq!(c_result, Err(EINVAL));
    assert!(!c.trace().is_empty(), "libusdr started erasing");
    let (r_result, r) = rsl_usdr(board(), |flash| {
        flash.programmer().write(offset, &data, WriteOptions::new())
    });
    assert!(matches!(r_result, Err(FlashError::EraseGranularity { .. })));
    assert!(r.trace().is_empty(), "nothing sent");
}

#[test]
fn writes_match_libusdr() {
    let master = MASTER_IMAGE_OFFSET;
    let cases = [
        // Erase first, then 32 chunks over 16 pages.
        (master, 0x1000, 0),
        // Erase two sectors' worth from a subsector, programming all but the header.
        (0xf000, 0x1_1000, DONT_WRITE_HEADER),
        // Onto unerased contents: programming only clears bits.
        (0x2000, 300, DONT_ERASE),
        (0x3010, 33, DONT_ERASE),
        (0x4003, 5, DONT_ERASE),
        (0x5000, 0x1000, DONT_ERASE | DONT_WRITE_HEADER),
    ];
    for (offset, len, flags) in cases {
        let data = image(len);
        let mut options = WriteOptions::new();
        if flags & DONT_ERASE != 0 {
            options = options.without_erase();
        }
        if flags & DONT_WRITE_HEADER != 0 {
            options = options.skip_header();
        }
        let (c_result, c) = libusdr(board(), |flash| flash.write(offset, &data, flags));
        let (r_result, r) = rsl_usdr(board(), |flash| {
            flash.programmer().write(offset, &data, options)
        });
        assert_eq!(
            c_result,
            Ok(()),
            "{offset:#x}+{len:#x} flags {flags}: libusdr writes"
        );
        assert!(
            r_result.is_ok(),
            "{offset:#x}+{len:#x} flags {flags}: {r_result:?}"
        );
        assert_same_effect(&c, &r);
    }
}

#[test]
fn writes_both_drivers_refuse_send_nothing() {
    let header = image(256);
    let cases: [(u32, &[u8], u32); 4] = [
        (0, &[], 0),
        (0, &[], DONT_ERASE),
        (0x1000, &header, DONT_ERASE | DONT_WRITE_HEADER),
        (0x800, &header, 0),
    ];
    for (offset, data, flags) in cases {
        let mut options = WriteOptions::new();
        if flags & DONT_ERASE != 0 {
            options = options.without_erase();
        }
        if flags & DONT_WRITE_HEADER != 0 {
            options = options.skip_header();
        }
        let (c_result, c) = libusdr(board(), |flash| flash.write(offset, data, flags));
        let (r_result, r) = rsl_usdr(board(), |flash| {
            flash.programmer().write(offset, data, options)
        });
        let len = data.len();
        assert_eq!(c_result, Err(EINVAL), "{offset:#x}+{len} flags {flags}");
        assert!(
            matches!(
                r_result,
                Err(FlashError::NothingToWrite { .. } | FlashError::EraseGranularity { .. })
            ),
            "{offset:#x}+{len} flags {flags}: {r_result:?}"
        );
        assert!(c.trace().is_empty() && r.trace().is_empty());
    }
}

/// Divergence: libusdr programs 128-byte chunks from any offset, so from one that is not a
/// multiple of 128 a chunk runs past its 256-byte page and the flash wraps the rest onto the
/// page's start; rsl-usdr refuses such a write before sending anything.
#[test]
fn a_chunk_crossing_a_page_is_refused_where_libusdr_corrupts_the_page() {
    let (offset, data) = (0x40, image(200));
    let (c_result, c) = libusdr(board(), |flash| flash.write(offset, &data, DONT_ERASE));
    assert_eq!(c_result, Ok(()), "libusdr reports success");
    let wrapped: Vec<u8> = pattern()[..8]
        .iter()
        .zip(&data[192..])
        .map(|(old, new)| old & new)
        .collect();
    assert_eq!(
        contents(&c)[..8],
        wrapped,
        "libusdr's second chunk wrapped its last 8 bytes onto the page start"
    );

    let (r_result, r) = rsl_usdr(board(), |flash| {
        flash
            .programmer()
            .write(offset, &data, WriteOptions::new().without_erase())
    });
    assert!(
        matches!(
            r_result,
            Err(FlashError::PageCrossing {
                addr: 0xc0,
                len: 72,
                ..
            })
        ),
        "{r_result:?}"
    );
    assert!(r.trace().is_empty(), "nothing sent");
}

#[test]
fn a_controller_that_stays_busy_times_out_like_libusdr() {
    let stuck = || board().with_stuck_flash_controller();
    let (reply, c) = libusdr(stuck(), FlashOracle::get_id);
    let (r_result, r) = rsl_usdr(stuck(), Flash::id);
    assert_eq!(reply.errno, ETIMEDOUT);
    assert!(
        matches!(r_result, Err(FlashError::ControllerTimeout)),
        "{r_result:?}"
    );
    assert_same_effect(&c, &r);
}

#[test]
fn a_flash_that_stays_busy_times_out_like_libusdr() {
    let stuck = || board_with(SimFlash::new(ID, SIZE).stuck_busy());
    let (c_result, c) = libusdr(stuck(), |flash| flash.erase(0x1000, 0x1000));
    let (r_result, r) = rsl_usdr(stuck(), |flash| flash.programmer().erase(0x1000, 0x1000));
    assert_eq!(c_result, Err(ETIMEDOUT));
    assert!(
        matches!(r_result, Err(FlashError::FlashTimeout)),
        "{r_result:?}"
    );
    assert_same_effect(&c, &r);
}
