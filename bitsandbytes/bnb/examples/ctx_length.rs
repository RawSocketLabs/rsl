//! **ctx_length** — `ctx`: a parent threads a value (here a column count) into its children,
//! including each element of a `count`-driven loop. A `ctx`-bearing type carries no length of
//! its own; it is decoded/encoded with `decode_with`/`to_bytes_with` + a generated `…Ctx`, not
//! the context-free `decode`/`to_bytes`. (A different `ctx` use than `ctx`'s off-wire `tag`
//! dispatch — here context sizes a field.)
//!
//! Run with: `cargo run -p bitsandbytes --example ctx_length`

use bnb::bin;

/// One row — its value count comes from the parent (the table's column count), not its bytes.
#[bin(big, ctx(columns: u8))]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Row {
    id: u16,
    #[br(count = columns)] // uses the ctx param
    values: Vec<u8>,
}

/// A table: a column count and a row count in the header, then rows that each omit their own
/// length — `columns` is threaded into every row via `ctx`.
#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Table {
    columns: u8,
    #[br(temp)]
    #[bw(calc = self.rows.len() as u8)]
    row_count: u8,
    #[br(count = row_count, ctx { columns })] // thread `columns` into each Row
    rows: Vec<Row>,
}

fn main() {
    let table = Table {
        columns: 3,
        rows: vec![
            Row {
                id: 1,
                values: vec![10, 20, 30],
            },
            Row {
                id: 2,
                values: vec![40, 50, 60],
            },
        ],
    };

    // `Table` has no context of its own (`columns` is a field), so it uses the plain entry
    // points; the *rows* are what need the threaded context.
    let bytes = table.to_bytes().unwrap();
    println!("table: {} bytes  {bytes:02x?}", bytes.len());
    assert_eq!(Table::decode_exact(&bytes).unwrap(), table);
    println!("{table:#?}");

    println!("all checks passed");
}
