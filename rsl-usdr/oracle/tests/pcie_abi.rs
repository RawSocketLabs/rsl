//! rsl-usdr-pcie's view of the `usdr_pcie_uram` driver interface against the C header, and
//! rsl-usdr's uSDR layout against the one libusdr's `PCIe` transport builds.
//!
//! The driver accepts a layout only once per module load and cannot report it back, so a
//! wrong byte would persist unnoticed on hardware; this is the check.

use rsl_usdr::transport::pcie::USDR_LAYOUT;
use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, SimBoard};

#[test]
fn sizes_offsets_and_request_codes_match_the_header() {
    let header = rsl_usdr_oracle::pcie_abi();
    let ours = rsl_usdr_pcie::abi();
    assert_eq!(header.len(), ours.len(), "the same facts are listed");
    for ((name, value), expected) in ours.iter().zip(&header) {
        assert_eq!(value, expected, "{name}");
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
