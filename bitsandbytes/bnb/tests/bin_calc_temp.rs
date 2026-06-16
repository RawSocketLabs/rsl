//! `calc`/`temp` (ROADMAP Phase 2, P2.4): `#[br(temp)] #[bw(calc = …)]` reads a
//! field into a local (usable by a later `count`) but does **not** store it —
//! `#[bin]` drops it from the struct and the builder — and recomputes it on write,
//! so the on-wire length can't drift from the `Vec`.

use bnb::{bin, u4};

#[bin]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Frame {
    tag: u4,
    #[br(temp)]
    #[bw(calc = self.items.len() as u16)]
    count: u16,
    #[br(count = count)]
    items: Vec<u8>,
}

#[test]
fn temp_field_is_dropped_and_recomputed() {
    // `count` is not a field — the struct literal omits it (compile-time proof),
    // and the on-wire count is recomputed from `items` on every encode.
    for n in [0usize, 1, 3, 7] {
        let f = Frame {
            tag: u4::new(0x5),
            items: vec![0xAB; n],
        };
        let bytes = f.to_bytes().unwrap();
        let decoded = Frame::decode_exact(&bytes).unwrap();
        assert_eq!(decoded, f);
        // Round-trip success across n proves the calc'd count matched items.len():
        // a wrong count would read the wrong number of elements.
        assert_eq!(decoded.items.len(), n);
    }
}

#[test]
fn builder_has_no_temp_field() {
    // The builder is over the cleaned struct, so it has `tag`/`items` but no
    // `count` (temp ⇒ not stored ⇒ not a builder field).
    let f = Frame::builder()
        .tag(u4::new(0xA))
        .items(vec![0x11, 0x22])
        .build()
        .unwrap();
    assert_eq!(Frame::decode_exact(&f.to_bytes().unwrap()).unwrap(), f);
}
