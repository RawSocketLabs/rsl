//! rsl-usdr-pcie's view of the `usdr_pcie_uram` driver interface against the C header, and
//! rsl-usdr's uSDR layout against the one libusdr's `PCIe` transport builds.
//!
//! The driver accepts a layout only once per module load and cannot report it back, so a
//! wrong byte would persist unnoticed on hardware; this is the check.

use std::collections::BTreeMap;

use rsl_usdr::transport::pcie::USDR_LAYOUT;
use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, SimBoard};

/// Every fact, matched by its C expression: the same names on both sides, and the same
/// value for each.
#[test]
fn sizes_offsets_and_request_codes_match_the_header() {
    let header: BTreeMap<String, u64> = rsl_usdr_oracle::pcie_abi().into_iter().collect();
    let ours: BTreeMap<String, u64> = rsl_usdr_pcie::abi()
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect();
    let names = |facts: &BTreeMap<String, u64>| facts.keys().cloned().collect::<Vec<_>>();
    assert_eq!(names(&ours), names(&header), "the same facts on both sides");
    for (name, value) in &ours {
        assert_eq!(*value, header[name], "{name}");
    }
}

#[test]
fn the_usdr_layout_matches_libusdrs() {
    let mut oracle =
        Oracle::open(SimBoard::new(BoardRevision::Rev3)).expect("libusdr opens the board");
    let expected = oracle
        .pcie_devlayout()
        .expect("libusdr describes the board");
    drop(oracle.close());
    let ours = USDR_LAYOUT.encode().expect("fits the driver's structure");
    let words = |bytes: &[u8]| -> Vec<u32> {
        bytes
            .chunks_exact(4)
            .map(|word| u32::from_ne_bytes(word.try_into().expect("4 bytes")))
            .collect()
    };
    let (expected, ours) = (words(&expected), words(&ours));
    if let Some(i) = expected.iter().zip(&ours).position(|(e, o)| e != o) {
        panic!(
            "first difference at byte {}: libusdr {:#x}, rsl-usdr {:#x}",
            i * 4,
            expected[i],
            ours[i]
        );
    }
}
