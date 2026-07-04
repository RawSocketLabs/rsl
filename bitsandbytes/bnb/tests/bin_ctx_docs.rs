//! Regression: the `…Ctx` struct that `#[bin(ctx(...))]` generates has **documented
//! fields**, so a consumer running `#![deny(missing_docs)]` compiles. (Surfaced by the
//! DNS port, where `RDataCtx`'s undocumented `rtype`/`rdlength` fields blocked the deny.)
//!
//! The `#![deny(missing_docs)]` at the top of this file IS the test: if the generated
//! `CellCtx` fields lacked docs, this would fail to compile.
#![deny(missing_docs)]

mod macro_ {
    use bnb::bin;

    /// A context-bearing child whose sizing and adjustment come from its parent.
    #[bin(big, ctx(width: u8, base: u16))]
    #[derive(Debug, PartialEq)]
    pub struct Cell {
        /// `width` payload bytes.
        #[br(count = width)]
        pub data: Vec<u8>,
        /// A checksum stored biased by `base`.
        #[br(map = |x: u16| x + base)]
        #[bw(map = |x: &u16| x - base)]
        pub checksum: u16,
    }

    #[test]
    fn ctx_type_decodes_under_deny_missing_docs() {
        // The generated `CellCtx` (with documented `width`/`base` fields) is usable here.
        let ctx = CellCtx::new(3, 10);
        let cell = Cell::decode_with_exact(&[0x01, 0x02, 0x03, 0x00, 0x14], ctx).unwrap();
        assert_eq!(cell.data, vec![0x01, 0x02, 0x03]);
        assert_eq!(cell.checksum, 0x14 + 10);
    }
}
