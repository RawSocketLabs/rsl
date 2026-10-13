//! `rsl-lms7002m`'s chip operations against libusdr's compiled `hw/lms7002m/lms7002m.c`,
//! word for word, from the zeroed state `xsdr_ctor` leaves: every bank selection, every
//! RFE/TRF path and mode, MIMO hand-over between channels, every AFE combination and both
//! processors. The XSDR session reaches only the disabling branches; these reach the rest.

use rsl_lms7002m::{Batch, Lms7002m, Mac, NoChannel, RxMode, RxPath, Tsp, TxMode, TxPath};
use rsl_xsdr::lowlevel::SpiAddr;
use rsl_xsdr_oracle::{Lms7002mCall as Call, lms7002m_run};
use rsl_xsdr_sim::{Op, SimBoard, Variant};

/// `EINVAL`: libusdr's refusal of a front-end call with no bank selected.
const EINVAL: i32 = 22;

/// libusdr's results and SPI words for `calls`.
fn libusdr(calls: &[Call]) -> (Vec<i32>, Vec<u32>) {
    let (results, board) = lms7002m_run(SimBoard::new(Variant::Xsdr), calls);
    let words = board
        .trace()
        .iter()
        .filter_map(|op| match op {
            Op::Spi { out, .. } => Some(*out),
            _ => None,
        })
        .collect();
    (results, words)
}

/// rsl-lms7002m's results and SPI words for `calls`, from [`Lms7002m::new`].
fn rsl(calls: &[Call]) -> (Vec<i32>, Vec<u32>) {
    let mut chip = Lms7002m::new(SpiAddr(0));
    let mut results = Vec::new();
    let mut words = Vec::new();
    for &call in calls {
        let batch: Result<Batch, NoChannel> = match call {
            Call::MacSet(mac) => Ok(chip.mac_set(mac_of(mac))),
            Call::RfePath(path, mode) => chip.rfe_path(rx_path(path), rx_mode(mode)),
            Call::TrfPath(path, mode) => chip.trf_path(tx_path(path), tx_mode(mode)),
            Call::AfeEnable([rxa, rxb, txa, txb]) => Ok(chip.afe_enable([rxa, rxb], [txa, txb])),
            Call::TspEnable(tsp, enable) => {
                Ok(chip.tsp_enable(if tsp == 1 { Tsp::Rx } else { Tsp::Tx }, enable))
            }
            Call::Destroy => Ok(chip.destroy()),
        };
        results.push(batch.map_or(-EINVAL, |batch| {
            words.extend(
                batch
                    .writes()
                    .iter()
                    .map(|&(reg, value)| 1 << 31 | u32::from(reg as u16) << 16 | u32::from(value)),
            );
            0
        }));
    }
    (results, words)
}

/// libusdr's `LMS7_CH_*` code as a bank selection.
fn mac_of(code: u32) -> Mac {
    [Mac::None, Mac::A, Mac::B, Mac::Both][code as usize]
}

/// libusdr's `lms7002m_rfe_path_t`.
fn rx_path(code: u32) -> RxPath {
    [RxPath::None, RxPath::LnaH, RxPath::LnaL, RxPath::LnaW][code as usize]
}

/// libusdr's `lms7002m_rfe_mode_t`.
fn rx_mode(code: u32) -> RxMode {
    [RxMode::Disable, RxMode::Normal, RxMode::LoopbackRf][code as usize]
}

/// libusdr's `lms7002m_trf_path_t`.
fn tx_path(code: u32) -> TxPath {
    [TxPath::Mute, TxPath::Band1, TxPath::Band2][code as usize]
}

/// libusdr's `lms7002m_trf_mode_t`.
fn tx_mode(code: u32) -> TxMode {
    [TxMode::Disable, TxMode::Normal, TxMode::Loopback][code as usize]
}

/// Panics, naming `scenario`, where the two disagree.
fn assert_same(calls: &[Call], scenario: &str) {
    let (expected, actual) = (libusdr(calls), rsl(calls));
    assert!(
        !expected.1.is_empty(),
        "{scenario}: libusdr wrote to the chip"
    );
    assert_eq!(actual.0, expected.0, "{scenario}: call results");
    if let Some(at) =
        (0..expected.1.len().max(actual.1.len())).find(|&i| expected.1.get(i) != actual.1.get(i))
    {
        panic!(
            "{scenario}: rsl-lms7002m diverges at word {at}: libusdr {:x?}, rsl-lms7002m {:x?}",
            expected.1.get(at),
            actual.1.get(at),
        );
    }
}

#[test]
fn every_bank_selection_path_and_mode_matches_libusdr() {
    for mac in 0..4 {
        for path in 0..4 {
            for mode in 0..3 {
                let calls = [
                    Call::MacSet(mac),
                    Call::RfePath(path, mode),
                    Call::TrfPath(path % 3, mode),
                    Call::Destroy,
                ];
                assert_same(&calls, &format!("MAC {mac}, path {path}, mode {mode}"));
            }
        }
    }
}

#[test]
fn a_channel_serving_the_other_matches_libusdr() {
    assert_same(
        &[
            Call::MacSet(3),
            Call::RfePath(1, 1),
            Call::TrfPath(2, 2),
            Call::MacSet(1),
            Call::RfePath(0, 0),
            Call::TrfPath(0, 0),
            Call::MacSet(2),
            Call::RfePath(3, 2),
            Call::TrfPath(1, 1),
        ],
        "channel B running while A is turned off",
    );
}

#[test]
fn every_afe_combination_and_both_processors_match_libusdr() {
    let afe: Vec<Call> = (0..16u8)
        .map(|bits| Call::AfeEnable(core::array::from_fn(|i| bits >> i & 1 == 1)))
        .collect();
    assert_same(&afe, "AFE");
    let tsp = [
        Call::TspEnable(0, true),
        Call::TspEnable(0, false),
        Call::TspEnable(1, true),
        Call::TspEnable(1, false),
    ];
    assert_same(&tsp, "processors");
}
