//! rsl-usdr's gateware update matches libusdr's flash tool (`tools/usdr_flash.c`) on the
//! simulated board: the same register operations and waits, in the same order, the same
//! verdict and the same flash contents afterwards, for each thing the tool does (`-w`,
//! `-w -G`, `-F`, `-k`, `-E`, `-i`) and each refusal it makes.
//!
//! The tool's device is not brought up or powered down around it (see `usdr_flash` in the
//! oracle); everything between is compared. One intentional divergence is whitelisted:
//! `a_corrupted_image_is_refused_before_rsl_usdr_reads_the_flash`. libusdr reports a
//! failed read-back and exits 0; rsl-usdr returns an error, which the oracle cannot show
//! (the simulated flash programs faithfully), so `tests/flash_update.rs` covers it.

#[path = "../../tests/common/bitstream.rs"]
#[allow(
    dead_code,
    reason = "each test crate uses part of the shared image builder"
)]
mod bitstream;
#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, PoisonError};

use bitstream::{AXSS, Bitstream, Facts, IDCODE, NOOP, WBSTAR, XC7A35T, XC7A50T};
use rsl_usdr::flash::{
    Flash, Image, ImageError, ImageMismatch, MASTER_IMAGE_OFFSET, Slot, UpdateError, UpdateOptions,
};
use rsl_usdr_oracle::flash::{ToolRun, usdr_flash};
use rsl_usdr_sim::{BoardRevision, Op, SimBoard, SimFlash};
use sim_bus::SimBus;

/// A Micron MT25QL032 (32 Mbit, 3.3 V): big enough for both slots.
const ID: u32 = 0x0016_ba20;
/// Its size, 4 MiB.
const SIZE: usize = 4 << 20;

/// The golden image's build on the flash.
const GOLDEN: u32 = 0x1111_0001;
/// The master image's build on the flash, and the one running.
const RUNNING: u32 = 0x2222_0002;
/// A new build.
const NEW: u32 = 0x3333_0003;

/// The tool's exit status for a refused image or an unreadable golden header.
const REFUSED: i32 = 4;
/// Its exit status for an image already flashed.
const ALREADY: i32 = 9;

/// A board running [`RUNNING`] whose flash holds `golden` and `master`, if given, over
/// `0xff`.
fn board(golden: Option<Facts>, master: Option<Facts>) -> SimBoard {
    let mut flash = SimFlash::new(ID, SIZE);
    if let Some(golden) = golden {
        flash = flash.with_contents(0, &golden.bytes());
    }
    if let Some(master) = master {
        let offset = usize::try_from(MASTER_IMAGE_OFFSET).expect("fits");
        flash = flash.with_contents(offset, &master.bytes());
    }
    SimBoard::new(BoardRevision::Rev3)
        .with_gateware_build(RUNNING)
        .with_flash(flash)
}

/// A board as shipped: golden [`GOLDEN`], master [`RUNNING`].
fn shipped() -> SimBoard {
    board(Some(Facts::golden(GOLDEN)), Some(Facts::master(RUNNING)))
}

/// Makes a fresh board for each driver.
type MakeBoard<'a> = &'a dyn Fn() -> SimBoard;

/// Whether rsl-usdr refused with the check libusdr's refusal corresponds to.
type Expected = fn(&ImageMismatch) -> bool;

/// The tool's arguments that pick `slot` (`-G` for the golden one).
fn slot_args(slot: Slot) -> &'static [&'static str] {
    match slot {
        Slot::Golden => &["-G"],
        Slot::Master => &[],
    }
}

/// Writes `bytes` to a file the tool can read, named for the test.
fn file(name: &str, bytes: &[u8]) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("flash_update_{name}.bin"));
    fs::write(&path, bytes).expect("writes the image file");
    path
}

/// Runs the tool on `board` with `args`, then the image file's path if given.
fn libusdr(board: SimBoard, args: &[&str], image: Option<&PathBuf>, reply: &str) -> ToolRun {
    let path = image.map(|path| path.to_str().expect("UTF-8 path").to_owned());
    let args: Vec<&str> = args.iter().copied().chain(path.as_deref()).collect();
    usdr_flash(board, &args, reply)
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

/// Requires both drivers to have done the same operations and left the same flash.
fn assert_same_effect(c: &ToolRun, rust: &SimBoard) {
    let (ct, rt) = (c.board.trace(), rust.trace());
    let at = ct
        .iter()
        .zip(rt)
        .position(|(c, r)| c != r)
        .or_else(|| (ct.len() != rt.len()).then(|| ct.len().min(rt.len())));
    if let Some(at) = at {
        panic!(
            "operation {at} differs: libusdr {:?}, rsl-usdr {:?} ({} and {} operations); \
             libusdr's log:\n{}",
            ct.get(at),
            rt.get(at),
            ct.len(),
            rt.len(),
            c.log
        );
    }
    assert!(
        contents(&c.board) == contents(rust),
        "the drivers left different flash contents"
    );
}

/// The board's flash contents.
fn contents(board: &SimBoard) -> &[u8] {
    board.flash().expect("the board has a flash").contents()
}

/// Whether the trace programs or erases anything: a write-enable command (`0x06` in the
/// command register's top byte, `espi_flash.c`:184-188).
fn changes_flash(trace: &[Op]) -> bool {
    trace
        .iter()
        .any(|op| matches!(op, Op::RegWrite { addr: 10, value } if value >> 24 == 0x06))
}

/// Updates `slot` of `board` with `facts` in both drivers, the tool with `args` (`-w` and
/// the file are added) and rsl-usdr with `options`, and requires the same effect.
fn update(
    name: &str,
    board: impl Fn() -> SimBoard,
    facts: Facts,
    slot: Slot,
    args: &[&str],
    options: UpdateOptions,
) -> (ToolRun, Result<(), UpdateError>) {
    let bytes = facts.bytes();
    let path = file(name, &bytes);
    let args: Vec<&str> = args.iter().copied().chain(["-w"]).collect();
    let c = libusdr(board(), &args, Some(&path), "");
    let image = Image::new(&bytes).expect("a valid image");
    let (rust, r) = rsl_usdr(board(), |flash| match slot {
        Slot::Golden => flash.programmer().update_golden(&image, options),
        Slot::Master => flash.programmer().update_master(&image, options),
    });
    assert_same_effect(&c, &r);
    (c, rust)
}

#[test]
fn a_master_update_matches_libusdr() {
    let facts = Facts::master(NEW);
    let (c, rust) = update(
        "master",
        shipped,
        facts,
        Slot::Master,
        &[],
        UpdateOptions::new(),
    );
    assert_eq!(c.status, 0, "libusdr updates: {}", c.log);
    assert!(
        c.log.contains("Write successful"),
        "libusdr verifies it: {}",
        c.log
    );
    rust.expect("rsl-usdr updates");
    let image = Image::new(&facts.bytes()).expect("valid");
    let offset = usize::try_from(MASTER_IMAGE_OFFSET).expect("fits");
    let written = &contents(&c.board)[offset..offset + image.as_bytes().len()];
    assert!(
        written == image.as_bytes(),
        "the master slot holds the padded image"
    );
    assert!(
        contents(&c.board)[..offset] == contents(&shipped())[..offset],
        "nothing below the master slot changed"
    );
}

#[test]
fn a_golden_update_matches_libusdr() {
    let (c, rust) = update(
        "golden",
        shipped,
        Facts::golden(NEW),
        Slot::Golden,
        &["-G"],
        UpdateOptions::new(),
    );
    assert_eq!(c.status, 0, "libusdr updates: {}", c.log);
    rust.expect("rsl-usdr updates");
}

#[test]
fn a_missing_master_image_does_not_stop_a_master_update() {
    let blank_master = || board(Some(Facts::golden(GOLDEN)), None);
    let (c, rust) = update(
        "no_master",
        blank_master,
        Facts::master(NEW),
        Slot::Master,
        &[],
        UpdateOptions::new(),
    );
    assert_eq!(c.status, 0, "libusdr updates: {}", c.log);
    rust.expect("rsl-usdr updates");
}

#[test]
fn builds_already_flashed_are_refused_like_libusdr_unless_forced() {
    // A master slot holding the new build, which does not run yet.
    let staged = || board(Some(Facts::golden(GOLDEN)), Some(Facts::master(NEW)));
    // `None`: the build runs from the master slot; else the slot that holds it.
    let cases: [(&str, MakeBoard<'_>, Facts, Slot, Option<Slot>); 4] = [
        (
            "again_master",
            &shipped,
            Facts::master(RUNNING),
            Slot::Master,
            None,
        ),
        (
            "running_golden",
            &shipped,
            Facts::golden(RUNNING),
            Slot::Golden,
            None,
        ),
        (
            "staged_master",
            &staged,
            Facts::master(NEW),
            Slot::Master,
            Some(Slot::Master),
        ),
        (
            "again_golden",
            &shipped,
            Facts::golden(GOLDEN),
            Slot::Golden,
            Some(Slot::Golden),
        ),
    ];
    for (name, board, facts, slot, flashed) in cases {
        let args = slot_args(slot);
        let (c, rust) = update(name, board, facts, slot, args, UpdateOptions::new());
        assert_eq!(c.status, ALREADY, "{name}: libusdr refuses: {}", c.log);
        assert!(
            !changes_flash(c.board.trace()),
            "{name}: libusdr changed nothing"
        );
        match (flashed, rust) {
            (None, Err(UpdateError::AlreadyRunning { build, .. })) => {
                assert_eq!(build, facts.build);
            }
            (
                Some(slot),
                Err(UpdateError::AlreadyFlashed {
                    slot: refused,
                    build,
                    ..
                }),
            ) => {
                assert_eq!((refused, build), (slot, facts.build), "{name}");
            }
            (_, other) => panic!("{name}: rsl-usdr refuses it as flashed, not {other:?}"),
        }

        let forced: Vec<&str> = args.iter().copied().chain(["-F"]).collect();
        let name = format!("{name}_forced");
        let reflash = UpdateOptions::new().reflash();
        let (c, rust) = update(&name, board, facts, slot, &forced, reflash);
        assert_eq!(c.status, 0, "{name}: libusdr writes it: {}", c.log);
        rust.unwrap_or_else(|error| panic!("{name}: rsl-usdr writes it: {error}"));
    }
}

#[test]
fn a_master_header_refused_after_its_build_still_counts_as_flashed_like_libusdr() {
    // The new build's stamp within the first 256 bytes, its WBSTAR past them: libusdr's
    // parse of the master header fails, but the build it read is still compared.
    let master = Bitstream::new()
        .write(IDCODE, &[XC7A35T])
        .write(AXSS, &[NEW])
        .raw(&[NOOP; 64])
        .write(WBSTAR, &[0])
        .bytes();
    let board = || {
        let offset = usize::try_from(MASTER_IMAGE_OFFSET).expect("fits");
        let flash = SimFlash::new(ID, SIZE)
            .with_contents(0, &Facts::golden(GOLDEN).bytes())
            .with_contents(offset, &master);
        SimBoard::new(BoardRevision::Rev3)
            .with_gateware_build(RUNNING)
            .with_flash(flash)
    };
    let facts = Facts::master(NEW);
    let (c, rust) = update(
        "partial_master",
        board,
        facts,
        Slot::Master,
        &[],
        UpdateOptions::new(),
    );
    assert_eq!(c.status, ALREADY, "libusdr refuses: {}", c.log);
    assert!(c.log.contains("M image is corrupted"), "{}", c.log);
    assert!(
        matches!(
            rust,
            Err(UpdateError::AlreadyFlashed {
                slot: Slot::Master,
                build: NEW,
                ..
            })
        ),
        "rsl-usdr compares the build it read: {rust:?}"
    );
}

#[test]
fn images_for_the_wrong_slot_are_refused_like_libusdr() {
    let unaligned = || {
        let golden = Facts {
            warm_boot_start: 0x30_0000,
            ..Facts::golden(GOLDEN)
        };
        board(Some(golden), Some(Facts::master(RUNNING)))
    };
    let other_fpga = Facts {
        device_id: XC7A50T,
        ..Facts::master(NEW)
    };
    let warm_booting_master = Facts {
        warm_boot_start: 0x30_0000,
        ..Facts::master(NEW)
    };
    let no_iprog = Facts {
        reprograms: false,
        ..Facts::golden(NEW)
    };
    let cases: [(&str, MakeBoard<'_>, Facts, Slot, Expected); 6] = [
        (
            "golden_for_master",
            &shipped,
            Facts::golden(NEW),
            Slot::Master,
            |m| matches!(m, ImageMismatch::GoldenImageForMaster),
        ),
        (
            "master_for_golden",
            &shipped,
            Facts::master(NEW),
            Slot::Golden,
            |m| matches!(m, ImageMismatch::NotAGoldenImage),
        ),
        (
            "golden_without_iprog",
            &shipped,
            no_iprog,
            Slot::Golden,
            |m| matches!(m, ImageMismatch::NotAGoldenImage),
        ),
        ("other_fpga", &shipped, other_fpga, Slot::Master, |m| {
            matches!(
                m,
                ImageMismatch::DeviceId {
                    flash: XC7A35T,
                    image: XC7A50T,
                    ..
                }
            )
        }),
        (
            "warm_booting_master",
            &shipped,
            warm_booting_master,
            Slot::Master,
            |m| {
                matches!(
                    m,
                    ImageMismatch::NotAMasterImage {
                        warm_boot_start: 0x30_0000,
                        ..
                    }
                )
            },
        ),
        (
            "unaligned_golden",
            &unaligned,
            Facts::master(NEW),
            Slot::Master,
            |m| {
                matches!(
                    m,
                    ImageMismatch::GoldenNotAligned {
                        warm_boot_start: 0x30_0000,
                        ..
                    }
                )
            },
        ),
    ];
    for (name, board, facts, slot, expected) in cases {
        let args = slot_args(slot);
        let (c, rust) = update(name, board, facts, slot, args, UpdateOptions::new());
        assert_eq!(c.status, REFUSED, "{name}: libusdr refuses: {}", c.log);
        assert!(c.log.contains("Image check failed"), "{name}: {}", c.log);
        assert!(
            !changes_flash(c.board.trace()),
            "{name}: libusdr changed nothing"
        );
        assert!(
            matches!(&rust, Err(UpdateError::Mismatch(found)) if expected(found)),
            "{name}: rsl-usdr refuses with the same check, not {rust:?}"
        );
    }
}

#[test]
fn an_unreadable_golden_header_stops_both() {
    let blank = || board(None, Some(Facts::master(RUNNING)));
    let (c, rust) = update(
        "blank_golden",
        blank,
        Facts::master(NEW),
        Slot::Master,
        &[],
        UpdateOptions::new(),
    );
    assert_eq!(c.status, REFUSED, "libusdr refuses: {}", c.log);
    assert!(c.log.contains("G image is corrupted"), "{}", c.log);
    assert!(
        matches!(rust, Err(UpdateError::GoldenHeader(_))),
        "rsl-usdr refuses: {rust:?}"
    );
}

#[test]
fn an_unrecognized_flash_stops_both() {
    let winbond = || {
        SimBoard::new(BoardRevision::Rev3)
            .with_gateware_build(RUNNING)
            .with_flash(SimFlash::new(0x0016_40ef, SIZE))
    };
    let (c, rust) = update(
        "winbond",
        winbond,
        Facts::master(NEW),
        Slot::Master,
        &[],
        UpdateOptions::new(),
    );
    assert_eq!(c.status, 2, "libusdr refuses: {}", c.log);
    assert!(
        matches!(rust, Err(UpdateError::UnrecognizedFlash(id)) if id.raw() == 0x0016_40ef),
        "rsl-usdr refuses: {rust:?}"
    );
}

#[test]
fn a_corrupted_image_is_refused_before_rsl_usdr_reads_the_flash() {
    // Whitelisted divergence: libusdr reads the board first, then refuses the file;
    // rsl-usdr refuses it when the image is made, before any bus access. Neither writes.
    let mut bytes = Facts::master(NEW).bytes();
    let last = bytes.len() / 2;
    bytes[last] ^= 0x10;
    let path = file("corrupted", &bytes);
    let c = libusdr(shipped(), &["-w"], Some(&path), "");
    assert_eq!(c.status, REFUSED, "libusdr refuses: {}", c.log);
    assert!(c.log.contains("file is corrupted"), "{}", c.log);
    assert!(!changes_flash(c.board.trace()), "libusdr changed nothing");
    assert!(
        matches!(Image::new(&bytes), Err(ImageError::Header(_))),
        "rsl-usdr refuses the image"
    );
    // What libusdr did first is what rsl-usdr reads to describe the board.
    let (images, r) = rsl_usdr(shipped(), Flash::boot_images);
    images.expect("reads");
    assert_same_effect(&c, &r);
}

#[test]
fn without_crc_checks_a_corrupted_image_is_flashed_like_libusdr() {
    let mut bytes = Facts::master(NEW).bytes();
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0x10;
    let path = file("unchecked", &bytes);
    let c = libusdr(shipped(), &["-k", "-w"], Some(&path), "");
    assert_eq!(c.status, 0, "libusdr writes it: {}", c.log);
    let image = Image::without_crc_check(&bytes).expect("the header is intact");
    let (rust, r) = rsl_usdr(shipped(), |flash| {
        flash
            .programmer()
            .update_master(&image, UpdateOptions::new())
    });
    rust.expect("rsl-usdr writes it");
    assert_same_effect(&c, &r);
}

#[test]
fn erasing_the_master_image_matches_libusdr() {
    let c = libusdr(shipped(), &["-E"], None, "YES\n");
    assert_eq!(c.status, 0, "libusdr erases: {}", c.log);
    let (rust, r) = rsl_usdr(shipped(), |flash| flash.programmer().erase_master());
    rust.expect("rsl-usdr erases");
    assert_same_effect(&c, &r);
    let offset = usize::try_from(MASTER_IMAGE_OFFSET).expect("fits");
    assert!(
        contents(&r)[offset..offset + 0x1_0000]
            .iter()
            .all(|&b| b == 0xff),
        "the master image's first sector is blank"
    );
}

#[test]
fn erasing_the_master_image_is_refused_like_libusdr_when_the_golden_image_does_not_boot_it() {
    for (warm_boot_start, status) in [(0, 7), (0x30_0000, 8)] {
        let golden = Facts {
            warm_boot_start,
            ..Facts::golden(GOLDEN)
        };
        let board = || board(Some(golden), Some(Facts::master(RUNNING)));
        let c = libusdr(board(), &["-E"], None, "YES\n");
        assert_eq!(c.status, status, "libusdr refuses: {}", c.log);
        let (rust, r) = rsl_usdr(board(), |flash| flash.programmer().erase_master());
        assert_same_effect(&c, &r);
        match rust {
            Err(UpdateError::NoMasterSupport) => assert_eq!(warm_boot_start, 0),
            Err(UpdateError::TrampolineMismatch {
                warm_boot_start: at,
                ..
            }) => {
                assert_eq!(at, warm_boot_start);
            }
            other => panic!("rsl-usdr refuses: {other:?}"),
        }
    }
}

#[test]
fn checking_an_image_matches_libusdrs_information_run() {
    for (name, facts, verdict) in [
        ("info_master", Facts::master(NEW), 0),
        ("info_golden", Facts::golden(NEW), REFUSED),
    ] {
        let bytes = facts.bytes();
        let path = file(name, &bytes);
        let c = libusdr(shipped(), &["-i"], Some(&path), "");
        assert_eq!(c.status, verdict, "{name}: {}", c.log);
        let image = Image::new(&bytes).expect("valid");
        let (images, r) = rsl_usdr(shipped(), Flash::boot_images);
        let check = images.expect("reads").check(&image, Slot::Master);
        assert_eq!(check.is_ok(), verdict == 0, "{name}: {check:?}");
        assert_same_effect(&c, &r);
    }
}
