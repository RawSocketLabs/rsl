//! **validate** — construction-side `#[bin(validate = path)]`: a predicate that gates `build()`,
//! also exposed as re-runnable `validate()` / `is_valid()`. The dual-use answer to binrw's
//! `pre_assert`: the **parser stays permissive** (decode never rejects representable input), so
//! validation is a *construction / pre-send* check, not a parser gate.
//!
//! Run with: `cargo run -p bitsandbytes --example validate`

use bnb::bin;

/// The invariant: the port window must be non-empty.
fn ports_sane(c: &Config) -> Result<(), String> {
    if c.min_port > c.max_port {
        Err(format!("min_port {} > max_port {}", c.min_port, c.max_port))
    } else {
        Ok(())
    }
}

#[bin(big, validate = ports_sane)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Config {
    min_port: u16,
    max_port: u16,
}

fn main() {
    // `build()` runs `validate` — a sound value builds.
    let c = Config::builder()
        .min_port(1000)
        .max_port(2000)
        .build()
        .unwrap();
    assert!(c.is_valid());
    println!("{c:?} -> is_valid = {}", c.is_valid());

    // An unsound value is rejected at `build()`.
    let bad = Config::builder().min_port(5000).max_port(1000).build();
    println!("bad build -> {bad:?}");
    assert!(bad.is_err());

    // ...but the PARSER stays permissive: the same bad bytes decode fine (dual-use)...
    let parsed = Config::decode_exact(&[0x13, 0x88, 0x03, 0xE8]).unwrap(); // min=5000, max=1000
    assert_eq!(parsed.min_port, 5000);
    // ...and you can re-check validity on demand (e.g. before acting on it).
    assert!(!parsed.is_valid());
    println!(
        "parsed (permissive) -> is_valid = {}, validate = {:?}",
        parsed.is_valid(),
        parsed.validate().map_err(|e| e.to_string())
    );

    println!("all checks passed");
}
