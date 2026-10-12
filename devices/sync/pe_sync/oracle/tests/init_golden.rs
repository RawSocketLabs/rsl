//! Characterization golden: libusdr's whole `PE_SYNC` session (open with full clock-tree
//! configuration, then close) on a fresh simulated board, operation for operation.
//!
//! This records what the pinned C does against the sim's modelled chips; it is not a
//! specification. It exists so the future `rsl-pe-sync` can be compared op-for-op against a
//! fixed reference: render its sim trace with `Op`'s `Display` and compare with the body.
//! A mismatch means libusdr, the sim or this harness changed. Re-bless only for a PIN bump
//! or a deliberate sim-model change, with `RSL_PE_SYNC_BLESS=1`, then review the fixture
//! diff and update [`CAPTURED`].

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use rsl_pe_sync_oracle::Oracle;
use rsl_pe_sync_sim::SimBoard;

/// When the fixture was last captured; written into a re-blessed header.
const CAPTURED: &str = "2026-10-12";
/// The fixture, under `tests/golden/`.
const FIXTURE: &str = "session.trace";

/// The fixture's path.
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(FIXTURE)
}

/// The vendored libusdr commit the golden was captured at.
fn pin() -> String {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../sdr/usdr/oracle/libusdr/PIN");
    fs::read_to_string(path)
        .expect("the vendored libusdr PIN is readable")
        .trim()
        .to_owned()
}

/// libusdr's session on a fresh board, one op per line, each phase opened by an
/// `== <phase>` line.
fn session() -> String {
    let oracle = Oracle::open(SimBoard::new()).expect("libusdr opens the board");
    let opened = oracle.trace_len();
    let board = oracle.close();

    let trace = board.trace();
    let mut body = String::new();
    for (phase, ops) in [("open", &trace[..opened]), ("close", &trace[opened..])] {
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
        "# rsl-pe-sync-oracle CHARACTERIZATION golden, not a specification: what the pinned\n\
         # libusdr does on rsl-pe-sync-sim's modelled board (see devices/sync/pe_sync/AGENTS.md).\n\
         # pin: {pin}\n\
         # captured: {CAPTURED}\n\
         # scenario: usdr_dmd_create_string(\"\") (power-on, status, clock tree), usdr_dmd_close.\n\
         # Re-bless (RSL_PE_SYNC_BLESS=1) only for a PIN bump or a deliberate sim-model change.\n\
         # Lines: rsl_pe_sync_sim::Op's Display; `== <phase>` opens each phase.\n",
        pin = pin(),
    )
}

#[test]
fn session_matches_its_characterization_golden() {
    let path = fixture();
    let actual = session();
    if env::var_os("RSL_PE_SYNC_BLESS").is_some() {
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
