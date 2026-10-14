//! Characterization golden: libusdr's whole LimeSDR-mini session (open, `/dm/power/en`,
//! close) on a fresh simulated board, operation for operation.
//!
//! This records what the pinned C does against the sim's modelled board; it is not a
//! specification. It exists so the future `limesdr` can be compared op-for-op against a
//! fixed reference: render its sim trace with `Op`'s `Display` and compare with the body.
//! `req`/`rep` lines are frames libusdr's `lms64c_proto.c` built and parsed, but which
//! command each low-level operation became is the oracle shim's assumption (the FT601
//! transport is not vendored); the other lines are the register operations libusdr asked
//! for. A mismatch means libusdr, the sim or this harness changed. Re-bless only for a PIN
//! bump or a deliberate sim-model change, with `RSL_LIMESDR_BLESS=1`, then review the
//! fixture diff and update [`CAPTURED`].

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use limesdr_oracle::Oracle;
use limesdr_sim::SimBoard;

/// When the fixture was last captured; written into a re-blessed header.
const CAPTURED: &str = "2026-10-12";
/// The fixture, under `tests/golden/`.
const FIXTURE: &str = "limesdr_mini_session.trace";

/// The fixture's path.
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(FIXTURE)
}

/// The vendored libusdr commit the golden was captured at.
fn pin() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../usdr/oracle/libusdr/PIN");
    fs::read_to_string(path)
        .expect("the vendored libusdr PIN is readable")
        .trim()
        .to_owned()
}

/// libusdr's session on a fresh board, one op per line, each phase opened by an
/// `== <phase>` line.
fn session() -> String {
    let mut oracle = Oracle::open(SimBoard::new()).expect("libusdr opens the LimeSDR-mini");
    let opened = oracle.trace_len();
    oracle.power_up().expect("libusdr accepts /dm/power/en");
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

/// The fixture's provenance header.
fn header() -> String {
    format!(
        "# limesdr-oracle CHARACTERIZATION golden, not a specification: what the pinned\n\
         # libusdr does on limesdr-sim's modelled board (see devices/sdr/limesdr/AGENTS.md).\n\
         # pin: {pin}\n\
         # board: LimeSDR-mini (U3_LIMESDR device ID), LMS7002M 0x3841, 40 MHz reference\n\
         # captured: {CAPTURED}\n\
         # scenario: usdr_dmd_create_string(\"\"), /dm/power/en = 1, usdr_dmd_close.\n\
         # req/rep frame encoding is the oracle shim's assumption (FT601 transport not vendored).\n\
         # Re-bless (RSL_LIMESDR_BLESS=1) only for a PIN bump or a deliberate sim-model change.\n\
         # Lines: limesdr_sim::Op's Display; `== <phase>` opens each phase.\n",
        pin = pin(),
    )
}

#[test]
fn limesdr_mini_session_matches_its_characterization_golden() {
    let path = fixture();
    let actual = session();
    if env::var_os("RSL_LIMESDR_BLESS").is_some() {
        fs::write(&path, header() + &actual).expect("the fixture is writable");
        return;
    }
    let golden = fs::read_to_string(&path).expect("the fixture exists; bless it once");
    let pin_line = format!("# pin: {}", pin());
    assert!(
        golden.lines().any(|line| line == pin_line),
        "{FIXTURE} was captured at another libusdr PIN; re-bless it in the PIN bump"
    );
    let expected: Vec<&str> = golden.lines().filter(|l| !l.starts_with('#')).collect();
    let actual: Vec<&str> = actual.lines().collect();
    if let Some(at) =
        (0..expected.len().max(actual.len())).find(|&i| expected.get(i) != actual.get(i))
    {
        panic!(
            "{FIXTURE}: libusdr's session diverges from the golden at op line {at}: \
             golden {:?}, now {:?}",
            expected.get(at),
            actual.get(at)
        );
    }
}
