//! **versioned_cells** — `ctx` + `try_map` together: a header `version` is **validated** by
//! `try_map` (rejecting an unknown version at decode) and **threaded** into each cell as context,
//! where it sets the cell's data width. A third `ctx` shape (after `ctx`'s off-wire tag dispatch
//! and `ctx_length`'s column count) and a third `try_map` (after `checked`/`versioned`).
//!
//! Run with: `cargo run -p bitsandbytes --example versioned_cells`

use bnb::{ErrorKind, bin};

/// Reject a version this build doesn't speak (a `try_map` parse-time guard).
fn check_version(raw: u8) -> Result<u8, String> {
    if (1..=2).contains(&raw) {
        Ok(raw)
    } else {
        Err(format!("unsupported version {raw}"))
    }
}

/// A cell whose data width comes from the parent's `version` context (v1 → 1 byte, v2 → 2).
#[bin(big, ctx(version: u8))]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Cell {
    tag: u8,
    #[br(count = version)]
    data: Vec<u8>,
}

#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Table {
    #[br(try_map = check_version)]
    #[bw(map = |v: &u8| *v)]
    version: u8,
    #[br(temp)]
    #[bw(calc = self.cells.len() as u8)]
    count: u8,
    #[br(count = count, ctx { version })] // thread the validated version into each cell
    #[nested]
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
