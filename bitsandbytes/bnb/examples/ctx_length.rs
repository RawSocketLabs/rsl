//! **ctx_length** — `ctx`: a parent threads a value (here a column count) into its children,
//! including each element of a `count`-driven loop. A `ctx`-bearing type carries no length of
//! its own; it is decoded/encoded with `decode_with`/`to_bytes_with` + a generated `…Ctx`, not
//! the context-free `decode`/`to_bytes`. (A different `ctx` use than `ctx`'s off-wire `tag`
//! dispatch — here context sizes a field.)
//!
//! Run with: `cargo run -p bitsandbytes --example ctx_length`

use bnb::{BitReader, bin};

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
///
/// **The ctx-count obligation.** `columns` sizes each row's `values` on *decode*; on
/// *encode* there is no ctx — every element a row holds is written as-is. Keeping
/// `values.len() == columns` is therefore the constructor's job, and `validate` is the
/// layer for it: it gates `build()` (and stays re-runnable) without ever touching the
/// parser. A mismatched value would otherwise encode bytes that don't round-trip —
/// which is also exactly what lets dual-use code forge such frames deliberately.
#[bin(big, validate = table_is_sound)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Table {
    columns: u8,
    #[brw(count_prefix = u8)] // the row count — sizes `rows`, recomputed on write
    #[br(ctx { columns })] // …and composes with threading `columns` into each Row
    rows: Vec<Row>,
}

/// Construction soundness: every row must hold exactly `columns` values.
fn table_is_sound(t: &Table) -> Result<(), String> {
    for (i, row) in t.rows.iter().enumerate() {
        if row.values.len() != t.columns as usize {
            return Err(format!(
                "row {i} has {} values; columns says {}",
                row.values.len(),
                t.columns
            ));
        }
    }
    Ok(())
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

    // A `ctx` type has no context-free entry points — decode one `Row` directly by passing the
    // generated `RowCtx` to `decode_with`. The Ctx is built positionally, in declaration order.
    let row_bytes = [0x00, 0x07, 0x0A, 0x14, 0x1E]; // id = 7, then `columns` (3) value bytes
    let mut reader = BitReader::new(&row_bytes);
    let row = Row::decode_with(&mut reader, RowCtx::new(3)).unwrap();
    assert_eq!(
        row,
        Row {
            id: 7,
            values: vec![10, 20, 30],
        }
    );

    // The obligation, enforced where it belongs: a row whose `values` disagree with
    // `columns` is caught by `validate` at build() — the parser stays permissive.
    let lopsided = Table::builder()
        .columns(3)
        .rows(vec![Row {
            id: 9,
            values: vec![1, 2], // 2 values under columns = 3
        }])
        .build();
    let err = lopsided.unwrap_err();
    println!("mismatched row -> {err}");

    println!("all checks passed");
}
