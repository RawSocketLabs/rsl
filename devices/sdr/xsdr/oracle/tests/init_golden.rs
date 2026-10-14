//! Characterization goldens: libusdr's whole LMS7 family session (open, RFIC power-up,
//! close) on a fresh simulated board, operation for operation.
//!
//! These record what the pinned C does against the sim's modelled chips; they are not a
//! specification. They exist so the future `xsdr` can be compared op-for-op against a
//! fixed reference: render its sim trace with `Op`'s `Display` and compare with the body.
//! A mismatch means libusdr, the sim or this harness changed. Re-bless only for a PIN bump
//! or a deliberate sim-model change, with `RSL_XSDR_BLESS=1`, then review the fixture diff
//! and update [`CAPTURED`].

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use xsdr_oracle::Oracle;
use xsdr_sim::{SimBoard, Variant};

/// When the fixtures were last captured; written into a re-blessed header.
const CAPTURED: &str = "2026-10-12";

/// A fixture file under `tests/golden/`.
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

/// The vendored libusdr commit the goldens were captured at.
fn pin() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../vendor/libusdr/PIN");
    fs::read_to_string(path)
        .expect("the vendored libusdr PIN is readable")
        .trim()
        .to_owned()
}

/// libusdr's session on a fresh `variant` board, one op per line, each phase opened by an
/// `== <phase>` line.
fn session(variant: Variant) -> String {
    let mut oracle = Oracle::open(SimBoard::new(variant)).expect("libusdr opens the board");
    let opened = oracle.trace_len();
    oracle.power_up().expect("libusdr powers the RFIC up");
    let powered = oracle.trace_len();
    let board = oracle.close();

    let trace = board.trace();
    let mut body = String::new();
    for (phase, ops) in [
        ("open", &trace[..opened]),
        ("power-up", &trace[opened..powered]),
        ("close", &trace[powered..]),
    ] {
        writeln!(body, "== {phase}").expect("invariant: writing to a String cannot fail");
        for op in ops {
            writeln!(body, "{op}").expect("invariant: writing to a String cannot fail");
        }
    }
    body
}

/// The provenance header of `variant`'s fixture.
fn header(variant: Variant, label: &str) -> String {
    format!(
        "# xsdr-oracle CHARACTERIZATION golden, not a specification: what the pinned\n\
         # libusdr does on xsdr-sim's modelled board (see devices/sdr/xsdr/AGENTS.md).\n\
         # pin: {pin}\n\
         # variant: {label} (HWID {hwid:#010x})\n\
         # captured: {CAPTURED}\n\
         # scenario: usdr_dmd_create_string(\"\"), /dm/power/en = 1, usdr_dmd_close.\n\
         # Re-bless (RSL_XSDR_BLESS=1) only for a PIN bump or a deliberate sim-model change.\n\
         # Lines: xsdr_sim::Op's Display; `== <phase>` opens each phase.\n",
        pin = pin(),
        hwid = u32::from(variant.device_id()) << 16,
    )
}

/// Compares `variant`'s session with its fixture, or rewrites the fixture when blessing.
fn assert_matches_golden(variant: Variant, label: &str, name: &str) {
    let path = fixture(name);
    let actual = session(variant);
    if env::var_os("RSL_XSDR_BLESS").is_some() {
        fs::write(&path, header(variant, label) + &actual).expect("the fixture is writable");
        return;
    }
    let golden = fs::read_to_string(&path).expect("the fixture exists; bless it once");
    let pin_line = format!("# pin: {}", pin());
    assert!(
        golden.lines().any(|line| line == pin_line),
        "{name} was captured at another libusdr PIN; re-bless it in the PIN bump"
    );
    let expected: Vec<&str> = golden.lines().filter(|l| !l.starts_with('#')).collect();
    let actual: Vec<&str> = actual.lines().collect();
    if let Some(at) =
        (0..expected.len().max(actual.len())).find(|&i| expected.get(i) != actual.get(i))
    {
        panic!(
            "{name}: libusdr's session diverges from the golden at op line {at}: \
             golden {:?}, now {:?}",
            expected.get(at),
            actual.get(at)
        );
    }
}

#[test]
fn xsdr_session_matches_its_characterization_golden() {
    assert_matches_golden(Variant::Xsdr, "XSDR", "xsdr_session.trace");
}

#[test]
fn xtrx_session_matches_its_characterization_golden() {
    assert_matches_golden(Variant::Xtrx, "XTRX revision 5", "xtrx_session.trace");
}
