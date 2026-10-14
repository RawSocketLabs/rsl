//! Characterization goldens: libusdr's whole `LSDr` session (open, ADC power-up, close) on a
//! fresh simulated board, operation for operation.
//!
//! These record what the pinned C does against the sim's modelled chips; they are not a
//! specification. They exist so the future `lsdr` can be compared op-for-op against a
//! fixed reference: render its sim trace with `Op`'s `Display` and compare with the body.
//! A mismatch means libusdr, the sim or this harness changed. Re-bless only for a PIN bump
//! or a deliberate sim-model change, with `RSL_LSDR_BLESS=1`, then review the fixture diff
//! and update [`CAPTURED`].

use std::env;
use std::ffi::CStr;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use lsdr_oracle::Oracle;
use lsdr_sim::{SimBoard, Variant};

/// When the fixtures were last captured; written into a re-blessed header.
const CAPTURED: &str = "2026-10-12";

/// One golden: the board, how libusdr is asked to open it, and where the fixture lives.
struct Scenario {
    /// The board fitted.
    variant: Variant,
    /// The connection string passed to `usdr_dmd_create_string`.
    connection: &'static CStr,
    /// The fit, as the fixture header names it.
    label: &'static str,
    /// The fixture file under `tests/golden/`.
    file: &'static str,
}

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

/// libusdr's session on a fresh board, one op per line, each phase opened by an
/// `== <phase>` line.
fn session(scenario: &Scenario) -> String {
    let mut oracle = Oracle::open(SimBoard::new(scenario.variant), scenario.connection)
        .expect("libusdr opens the board");
    let opened = oracle.trace_len();
    oracle.set_power(true).expect("libusdr powers the ADCs up");
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

/// The provenance header of a scenario's fixture.
fn header(scenario: &Scenario) -> String {
    format!(
        "# lsdr-oracle CHARACTERIZATION golden, not a specification: what the pinned\n\
         # libusdr does on lsdr-sim's modelled board (see devices/sdr/lsdr/AGENTS.md).\n\
         # pin: {pin}\n\
         # variant: {label}\n\
         # captured: {CAPTURED}\n\
         # scenario: usdr_dmd_create_string({connection:?}), /dm/power/en = 1, usdr_dmd_close.\n\
         # Re-bless (RSL_LSDR_BLESS=1) only for a PIN bump or a deliberate sim-model change.\n\
         # Lines: lsdr_sim::Op's Display; `== <phase>` opens each phase.\n",
        pin = pin(),
        label = scenario.label,
        connection = scenario.connection.to_string_lossy(),
    )
}

/// Compares a scenario's session with its fixture, or rewrites the fixture when blessing.
fn assert_matches_golden(scenario: &Scenario) {
    let path = fixture(scenario.file);
    let actual = session(scenario);
    if env::var_os("RSL_LSDR_BLESS").is_some() {
        fs::write(&path, header(scenario) + &actual).expect("the fixture is writable");
        return;
    }
    let name = scenario.file;
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
fn single_adc_session_matches_its_characterization_golden() {
    assert_matches_golden(&Scenario {
        variant: Variant::SingleAdc,
        connection: c"",
        label: "single ADC (registered s_uuid_000 board), default connection",
        file: "single_adc_session.trace",
    });
}

#[test]
fn dual_adc_clock_gen_session_matches_its_characterization_golden() {
    assert_matches_golden(&Scenario {
        variant: Variant::DualAdcClockGen,
        connection: c"lmk=1",
        label: "dual ADC + LMK04832 clock generator, LMK probe requested",
        file: "dual_adc_clock_gen_session.trace",
    });
}
