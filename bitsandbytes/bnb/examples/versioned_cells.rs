//! **versioned_cells** — `ctx` + a decode-time guard together: a header `version` is **guarded**
//! by `#[br(assert(...))]` (rejecting an unknown version at decode) and **threaded** into each
//! cell as context, where it sets the cell's data width. A third `ctx` shape (after `ctx`'s
//! off-wire tag dispatch and `ctx_length`'s column count).
//!
//! Run with: `cargo run -p bitsandbytes --example versioned_cells`

use bnb::{ErrorKind, bin};

/// A cell whose data width comes from the parent's `version` context (v1 → 1 byte, v2 → 2).
#[bin(big, ctx(version: u8))]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Cell {
    tag: u8,
    // `version` sizes `data` on DECODE only; encode writes whatever `data` holds (there
    // is no ctx at encode). Keeping `data.len() == version` is the constructor's job —
    // see `ctx_length` for the `validate`-enforced version of this obligation.
    #[br(count = version)]
    data: Vec<u8>,
}

#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Table {
    // The decode-time guard: reject a version this build doesn't speak (read-only —
    // no write inverse needed; encode still emits whatever is stored).
    #[br(assert((1..=2).contains(&version), "unsupported version {}", version))]
    version: u8,
    #[brw(count_prefix = u8)] // the u8 count prefix — derived, never stored, checked at encode
    #[br(ctx { version })] // thread the validated version into each cell
    cells: Vec<Cell>,
}

fn main() {
    let table = Table {
        version: 2,
        cells: vec![
            Cell {
                tag: 0x10,
                data: vec![0xAA, 0xBB],
            },
            Cell {
                tag: 0x20,
                data: vec![0xCC, 0xDD],
            },
        ],
    };
    let bytes = table.to_bytes().unwrap();
    println!("{table:#?}");
    println!("  -> {} bytes  {bytes:02x?}", bytes.len());
    assert_eq!(Table::decode_exact(&bytes).unwrap(), table);

    // Version 9 isn't supported — `try_map` rejects it at decode, before any cell is read.
    let err = Table::decode_exact(&[0x09, 0x00]).unwrap_err();
    println!("decoding version=9 -> {err}");
    assert!(matches!(err.kind, ErrorKind::Convert { .. }));

    println!("all checks passed");
}
