//! The ledger and the code agree: every `prov:` tag in `rtlsdr` and `rtlsdr-sim` names a
//! `PROVENANCE.md` row, every DS row is tagged somewhere, every GAP row that names code is
//! referenced by it, and every identifier a row's Rust-item column names exists.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Every `.rs` file under `dir`.
fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("source directory") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The non-test code of both crates, concatenated.
fn code() -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    sources(&root.join("src"), &mut files);
    sources(&root.join("sim/src"), &mut files);
    files
        .iter()
        .map(|path| fs::read_to_string(path).expect("readable source"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether `c` can be part of a ledger id or identifier.
fn word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// The ids following `prov: ` in `code`.
fn tags(code: &str) -> BTreeSet<String> {
    code.match_indices("prov: ")
        .map(|(at, _)| {
            code[at + 6..]
                .chars()
                .take_while(|&c| word_char(c))
                .collect::<String>()
        })
        // Prose that describes the tag (`prov: <ID>`) is not one.
        .filter(|tag| !tag.is_empty())
        .collect()
}

/// One ledger row: its id, Rust-item column and class.
struct Row {
    /// `R820-DS-01` and the like.
    id: String,
    /// The Rust-item column.
    item: String,
    /// DS, GAP, …
    class: String,
}

/// The ledger's rows.
fn rows() -> Vec<Row> {
    let ledger = include_str!("../PROVENANCE.md");
    ledger
        .lines()
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            let id = *cells.get(1)?;
            (id.starts_with("R820-") || id.starts_with("RTL-")).then(|| Row {
                id: id.to_owned(),
                item: cells.get(2).copied().unwrap_or_default().to_owned(),
                class: cells.get(5).copied().unwrap_or_default().to_owned(),
            })
        })
        .collect()
}

/// The identifiers inside the backticked spans of `cell`.
fn identifiers(cell: &str) -> Vec<String> {
    cell.split('`')
        .skip(1)
        .step_by(2)
        .flat_map(|span| {
            span.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .filter(|word| word.chars().next().is_some_and(|c| c.is_ascii_alphabetic()))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn every_prov_tag_names_a_ledger_row() {
    let ids: BTreeSet<_> = rows().into_iter().map(|row| row.id).collect();
    let missing: Vec<_> = tags(&code())
        .into_iter()
        .filter(|tag| !ids.contains(tag))
        .collect();
    assert!(missing.is_empty(), "tags without ledger rows: {missing:?}");
}

#[test]
fn every_ds_row_is_tagged_and_every_gap_naming_code_is_referenced() {
    let code = code();
    let tags = tags(&code);
    for row in rows() {
        match row.class.as_str() {
            "DS" => assert!(tags.contains(&row.id), "{} has no prov tag in code", row.id),
            "GAP" if !row.item.starts_with("none") => {
                assert!(
                    code.contains(&row.id),
                    "{} names code that never cites it",
                    row.id
                );
            }
            "GAP" => {}
            other => panic!("{}: class {other} has no admissible source yet", row.id),
        }
    }
}

#[test]
fn every_item_a_row_names_exists() {
    let code = code();
    for row in rows() {
        for identifier in identifiers(&row.item) {
            let exists = code.match_indices(identifier.as_str()).any(|(at, _)| {
                let before = code[..at].chars().next_back().is_none_or(|c| !word_char(c));
                let after = code[at + identifier.len()..]
                    .chars()
                    .next()
                    .is_none_or(|c| !word_char(c));
                before && after
            });
            assert!(
                exists,
                "{} names `{identifier}`, which the code lacks",
                row.id
            );
        }
    }
}
