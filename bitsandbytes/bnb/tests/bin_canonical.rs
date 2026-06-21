//! In-memory canonicalization helpers, generated alongside `to_canonical_bytes` when a
//! message has a `reserved` or non-`temp` `calc` field:
//!   * `to_canonical(self) -> Self` — reserved → spec value, `calc` → recomputed;
//!   * `canonical_diff(&self) -> Vec<&'static str>` — names of fields that aren't canonical;
//!   * `is_canonical(&self) -> bool`.

use bnb::{EncodeExt, EncodeMode, bin, u4};

#[bin(big)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Msg {
    tag: u4,
    #[reserved]
    rsv: u4, // spec value: 0
    #[bw(calc = self.payload.wrapping_add(1))]
    #[builder(default)]
    check: u8, // canonical value: payload + 1
    payload: u8,
}

#[test]
fn to_canonical_recomputes_calc_and_normalizes_reserved() {
    // A deliberately non-canonical value: reserved bits set, a stale checksum. (Construction
    // is builder-only now — the injected `encode_mode` field can't be named in a literal.)
    let m = Msg::builder()
        .tag(u4::new(0xA))
        .rsv(u4::new(0xF))
        .check(0x99)
        .payload(0x10)
        .build()
        .unwrap();

    // Verbatim `to_bytes` keeps the bogus values exactly (E1's verbatim contract).
    assert_eq!(m.to_bytes().unwrap(), [0xAF, 0x99, 0x10]);

    // `canonical_diff` names the off fields; `is_canonical` is false.
    let mut diff = m.canonical_diff();
    diff.sort_unstable();
    assert_eq!(diff, ["check", "rsv"]);
    assert!(!m.is_canonical());

    // `to_canonical` produces the canonical form in memory.
    let c = m.clone().to_canonical();
    assert_eq!(c.rsv, u4::new(0)); // reserved → spec value
    assert_eq!(c.check, 0x11); // calc → payload + 1
    assert_eq!(c.tag, u4::new(0xA)); // ordinary fields unchanged
    assert_eq!(c.payload, 0x10);
    assert!(c.is_canonical());
    assert!(c.canonical_diff().is_empty());

    // The defining identity: `x.to_canonical().to_bytes() == x.to_canonical_bytes()`.
    assert_eq!(c.to_bytes().unwrap(), m.to_canonical_bytes().unwrap());
    assert_eq!(m.to_canonical_bytes().unwrap(), [0xA0, 0x11, 0x10]);
}

#[test]
fn an_already_canonical_value_has_no_diff() {
    let m = Msg::builder()
        .tag(u4::new(0x3))
        .payload(0x20)
        .build()
        .unwrap(); // builder defaults rsv (spec) and omits check entirely
    // The builder default leaves `check` at 0, which is *not* payload+1 (0x21), so the
    // freshly-built value isn't canonical until canonicalized.
    assert_eq!(m.canonical_diff(), ["check"]);
    let c = m.to_canonical();
    assert!(c.is_canonical());
    assert_eq!(c.check, 0x21);
}

#[test]
fn encode_follows_the_values_mode() {
    // The std-writer `encode(w)` follows the value's `encode_mode`; the Vec methods stay
    // explicit (`to_bytes` verbatim, `to_canonical_bytes` canonical).
    let m = Msg::builder()
        .tag(u4::new(0xA))
        .rsv(u4::new(0xF))
        .check(0x99)
        .payload(0x10)
        .build()
        .unwrap();

    // The builder defaults the mode to Verbatim.
    assert_eq!(m.encode_mode(), EncodeMode::Verbatim);
    let mut verbatim = Vec::new();
    m.encode(&mut verbatim).unwrap();
    assert_eq!(verbatim, m.to_bytes().unwrap());

    // `with_encode_mode(Canonical)` → `encode` now writes the canonical form.
    let mut canonical = Vec::new();
    m.clone()
        .with_encode_mode(EncodeMode::Canonical)
        .encode(&mut canonical)
        .unwrap();
    assert_eq!(canonical, m.to_canonical_bytes().unwrap());
    assert_ne!(verbatim, canonical); // reserved + stale calc → the two forms differ

    // `set_encode_mode` (in place) and the builder's `.encode_mode(…)` reach the same state.
    let mut m2 = m.clone();
    m2.set_encode_mode(EncodeMode::Canonical);
    assert_eq!(m2.encode_mode(), EncodeMode::Canonical);
    let built = Msg::builder()
        .tag(u4::new(0xA))
        .payload(0x10)
        .encode_mode(EncodeMode::Canonical)
        .build()
        .unwrap();
    assert_eq!(built.encode_mode(), EncodeMode::Canonical);

    // The mode is excluded from equality: two values differing only in mode are equal.
    assert_eq!(m, m.clone().with_encode_mode(EncodeMode::Canonical));

    // A decoded value defaults to Verbatim (so decode -> encode round-trips).
    let decoded = Msg::decode_exact(&verbatim).unwrap();
    assert_eq!(decoded.encode_mode(), EncodeMode::Verbatim);
    let mut re = Vec::new();
    decoded.encode(&mut re).unwrap();
    assert_eq!(re, verbatim);
}

#[test]
fn new_constructs_from_all_stored_fields() {
    // `new` is the literal replacement: every stored field in declaration order (reserved +
    // calc included), and the encode mode starts at Verbatim.
    let m = Msg::new(u4::new(0xA), u4::new(0xF), 0x99, 0x10);
    assert_eq!(m.encode_mode(), EncodeMode::Verbatim);
    assert_eq!(m.to_bytes().unwrap(), [0xAF, 0x99, 0x10]);

    // Same value as the all-fields-set builder.
    let b = Msg::builder()
        .tag(u4::new(0xA))
        .rsv(u4::new(0xF))
        .check(0x99)
        .payload(0x10)
        .build()
        .unwrap();
    assert_eq!(m, b);
}

#[test]
fn mode_is_excluded_from_debug() {
    let m = Msg::builder()
        .tag(u4::new(1))
        .payload(2)
        .encode_mode(EncodeMode::Canonical)
        .build()
        .unwrap();
    let s = format!("{m:?}");
    assert!(
        !s.contains("encode_mode"),
        "Debug must not show the render-preference field: {s}"
    );
    assert!(s.contains("tag") && s.contains("payload"), "{s}");
}
