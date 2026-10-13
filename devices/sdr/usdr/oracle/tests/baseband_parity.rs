//! rsl-usdr's per-channel LOs and baseband offsets against libusdr's
//! `/dm/sdr/0/{rx,tx}/frequency[/0|/1]` and `/dm/sdr/0/{rx,tx}/frequency/bb[/0|/1]`,
//! operation for operation, with what they leave for later rate changes and tunes.

#[path = "common/crossing.rs"]
mod crossing;
#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::ffi::CString;
use std::ops::Range;

use crossing::{Crossing, with_band_crossing_fix};
use rsl_usdr::{Device, Error, NcoChannels};
use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, Op, PllLock, SimBoard};
use sim_bus::SimBus;

/// A chain.
#[derive(Clone, Copy, Debug)]
enum Chain {
    /// The receiver.
    Rx,

    /// The transmitter.
    Tx,
}

/// One call, made the same way on both sides.
#[derive(Clone, Copy, Debug)]
enum Call {
    /// libusdr's `{rate, 0}`; rsl-usdr's `set_rx_sample_rate`.
    Rate(u32),

    /// libusdr's `/dm/sdr/0/{rx,tx}/frequency[/0|/1]`; rsl-usdr's
    /// `set_{rx,tx}_channel_frequency`.
    Frequency(Chain, NcoChannels, u32),

    /// libusdr's `/dm/sdr/0/{rx,tx}/frequency/bb[/0|/1]`; rsl-usdr's
    /// `set_{rx,tx}_baseband_offset`.
    Baseband(Chain, NcoChannels, i32),
}

/// libusdr's path suffix for `channels`.
fn suffix(channels: NcoChannels) -> &'static str {
    match channels {
        NcoChannels::Channel0 => "/0",
        NcoChannels::Channel1 => "/1",
        _ => "",
    }
}

/// libusdr's chain directory.
fn chain_dir(chain: Chain) -> &'static str {
    match chain {
        Chain::Rx => "rx",
        Chain::Tx => "tx",
    }
}

impl Call {
    /// Makes the call through libusdr, returning whether it succeeded.
    fn on_libusdr(self, oracle: &mut Oracle) -> bool {
        let (path, value) = match self {
            Self::Rate(rate) => return oracle.set_rx_rate(rate).is_ok(),
            Self::Frequency(chain, channels, hz) => (
                format!(
                    "/dm/sdr/0/{}/frequency{}",
                    chain_dir(chain),
                    suffix(channels)
                ),
                u64::from(hz),
            ),
            Self::Baseband(chain, channels, hz) => (
                format!(
                    "/dm/sdr/0/{}/frequency/bb{}",
                    chain_dir(chain),
                    suffix(channels)
                ),
                // libusdr reads the value back as `int64_t`.
                u64::from_ne_bytes(i64::from(hz).to_ne_bytes()),
            ),
        };
        let path = CString::new(path).expect("no NUL");
        // SAFETY: libusdr's frequency and baseband handlers (`dev_m2_lm6_1_sdr_*_freq_set`,
        // `..._freqbb_set`) read their value as a number.
        unsafe { oracle.set_uint(&path, value) }.is_ok()
    }

    /// Makes the call through rsl-usdr.
    fn on_rsl(self, device: &mut Device) -> Result<(), Error> {
        match self {
            Self::Rate(rate) => device.set_rx_sample_rate(rate),
            Self::Frequency(Chain::Rx, channels, hz) => {
                device.set_rx_channel_frequency(channels, hz)
            }
            Self::Frequency(Chain::Tx, channels, hz) => {
                device.set_tx_channel_frequency(channels, hz)
            }
            Self::Baseband(Chain::Rx, channels, hz) => device.set_rx_baseband_offset(channels, hz),
            Self::Baseband(Chain::Tx, channels, hz) => device.set_tx_baseband_offset(channels, hz),
        }
    }
}

/// The RX chain's configuration port (`REG_CFG_PHY_0`).
const REG_RX_PHY: u32 = 56;

/// The NCO words, channel 0 then 1, a window last wrote to configuration port `port`
/// (sub-registers 8 and 9 for NCO 0's low and high halves, 10 and 11 for NCO 1's).
fn nco_words(window: &[Op], port: u32) -> [u32; 2] {
    let mut halves = [0_u32; 4];
    for op in window {
        if let Op::RegWrite { addr, value } = op
            && *addr == port
            && (8..12).contains(&(value >> 24))
        {
            halves[usize::try_from((value >> 24) - 8).expect("fits")] = value & 0xffff;
        }
    }
    [halves[1] << 16 | halves[0], halves[3] << 16 | halves[2]]
}

/// A revision-3 board.
fn rev3() -> SimBoard {
    SimBoard::new(BoardRevision::Rev3)
}

/// A revision-3 board whose PLLs cannot lock below 245 MHz.
fn rev3_unlocked_below_245m() -> SimBoard {
    let lock = PllLock {
        unlocked_below_hz: 245_000_000,
        ..PllLock::default()
    };
    rev3().with_rx_pll_lock(lock).with_tx_pll_lock(lock)
}

/// Makes `calls` in turn on fresh boards from `board` through libusdr and rsl-usdr and
/// compares each call's outcome and operations, with libusdr's band crossings replaced by
/// rsl-usdr's fix where `crossing` expects them; returns whether each succeeded, with its
/// operations.
fn assert_calls_match(
    board: impl Fn() -> SimBoard,
    crossing: Crossing,
    calls: &[Call],
) -> Vec<(bool, Vec<Op>)> {
    let mut oracle = Oracle::open(board()).expect("libusdr opens the board");
    let mut expected: Vec<(bool, Range<usize>)> = Vec::new();
    for &call in calls {
        let start = oracle.trace_len();
        let ok = call.on_libusdr(&mut oracle);
        expected.push((ok, start..oracle.trace_len()));
    }
    let reference = oracle.close();

    let sim = SimBus::new(board());
    let mut device = Device::with_bus(sim.clone()).expect("rsl-usdr powers up the board");
    let mut actual: Vec<(Result<(), Error>, Range<usize>)> = Vec::new();
    for &call in calls {
        let start = sim.board().trace().len();
        let result = call.on_rsl(&mut device);
        actual.push((result, start..sim.board().trace().len()));
    }
    let board = sim.board();

    let mut outcomes = Vec::new();
    for ((call, (expected_ok, expected)), (result, actual)) in
        calls.iter().zip(expected).zip(actual)
    {
        assert_eq!(
            result.is_ok(),
            expected_ok,
            "{call:?}: libusdr ok={expected_ok}, rsl-usdr {result:?}"
        );
        let mut expected = reference.trace()[expected].to_vec();
        if crossing == Crossing::Fixed {
            (expected, _) = with_band_crossing_fix(&expected);
        }
        let actual: &[Op] = &board.trace()[actual];
        if let Some(i) = expected.iter().zip(actual).position(|(e, a)| e != a) {
            panic!(
                "{call:?}: first difference at op {i}: libusdr {:x?}, rsl-usdr {:x?}",
                expected[i], actual[i]
            );
        }
        assert_eq!(
            expected.len(),
            actual.len(),
            "{call:?}: same prefix, different lengths"
        );
        outcomes.push((expected_ok, expected));
    }
    outcomes
}

/// RX offsets for both channels, then each alone, survive a rate change (both NCOs
/// restored with their own offsets, the filter wider by the largest) until a whole tune
/// sets them back to 0.
#[test]
fn rx_baseband_offsets_match_libusdr() {
    use NcoChannels::{Both, Channel0, Channel1};
    let outcomes = assert_calls_match(
        rev3,
        Crossing::None,
        &[
            Call::Rate(20_000_000),
            Call::Frequency(Chain::Rx, Both, 1_000_000_000),
            Call::Baseband(Chain::Rx, Both, 1_000_000),
            Call::Baseband(Chain::Rx, Channel1, -2_500_000),
            Call::Baseband(Chain::Rx, Channel0, 0),
            Call::Rate(10_000_000),
            Call::Frequency(Chain::Rx, Both, 900_000_000),
            Call::Rate(20_000_000),
        ],
    );
    assert!(outcomes.iter().all(|(ok, _)| *ok), "every call succeeds");
    // At a 40 MS/s converter rate (`_usdr_set_nco`): +1 MHz is 0.025 of `INT_MAX`,
    // truncated and doubled; -2.5 MHz -0.0625 of it. Each offset reaches its own NCO.
    let words = |call: usize| nco_words(&outcomes[call].1, REG_RX_PHY);
    assert_eq!(words(2), [0x0666_6666, 0x0666_6666], "both channels +1 MHz");
    assert_eq!(
        words(3),
        [0x0666_6666, 0xf000_0002],
        "channel 1 at -2.5 MHz"
    );
    assert_eq!(words(6), [0, 0], "a whole tune clears both offsets");
}

/// Two RX channel LOs centre the PLL between them; an offset set afterwards replaces one
/// channel's distance until that channel's LO is set again.
#[test]
fn rx_channel_frequencies_match_libusdr() {
    use NcoChannels::{Channel0, Channel1};
    let outcomes = assert_calls_match(
        rev3,
        Crossing::None,
        &[
            Call::Rate(20_000_000),
            Call::Frequency(Chain::Rx, Channel0, 1_000_000_000),
            Call::Frequency(Chain::Rx, Channel1, 1_004_000_001),
            Call::Baseband(Chain::Rx, Channel0, 500_000),
            Call::Rate(10_000_000),
            Call::Frequency(Chain::Rx, Channel0, 999_000_000),
        ],
    );
    assert!(outcomes.iter().all(|(ok, _)| *ok), "every call succeeds");
}

/// The same for TX: offsets, channel LOs and a rate change that restores both NCOs.
#[test]
fn tx_baseband_offsets_and_channel_frequencies_match_libusdr() {
    use NcoChannels::{Both, Channel0, Channel1};
    let outcomes = assert_calls_match(
        rev3,
        Crossing::None,
        &[
            Call::Rate(20_000_000),
            Call::Frequency(Chain::Tx, Both, 1_000_000_000),
            Call::Baseband(Chain::Tx, Channel0, 1_000_000),
            Call::Baseband(Chain::Tx, Channel1, -3_000_000),
            Call::Frequency(Chain::Tx, Channel1, 1_010_000_000),
            Call::Rate(10_000_000),
            Call::Baseband(Chain::Tx, Both, -250_000),
        ],
    );
    assert!(outcomes.iter().all(|(ok, _)| *ok), "every call succeeds");
}

/// Offsets ride on the low-LO fallback's NCO shift: below 250 MHz the PLL settles at its
/// lowest lock (245 MHz in this sim) and each NCO adds the fallback's gap to its own
/// offset, which 1 MS/s (a 32 MS/s converter) has room for.
#[test]
fn baseband_offsets_add_to_the_low_lo_fallback_like_libusdr() {
    use NcoChannels::{Channel0, Channel1};
    let outcomes = assert_calls_match(
        rev3_unlocked_below_245m,
        Crossing::None,
        &[
            Call::Rate(1_000_000),
            Call::Frequency(Chain::Rx, Channel0, 240_000_000),
            Call::Baseband(Chain::Rx, Channel1, 2_000_000),
            Call::Frequency(Chain::Tx, Channel1, 240_000_000),
            Call::Baseband(Chain::Tx, Channel0, -2_000_000),
        ],
    );
    assert!(outcomes.iter().all(|(ok, _)| *ok), "every call succeeds");
}

/// An offset before any tune retunes an LO of 0: libusdr and rsl-usdr fail or succeed
/// alike, with the same operations. The receiver's 0 Hz sits in the mixer band, so the
/// scenario takes the band-crossing fix (whitelisted divergence).
#[test]
fn offsets_before_any_tune_match_libusdr() {
    use NcoChannels::Both;
    assert_calls_match(
        rev3,
        Crossing::Fixed,
        &[
            Call::Rate(20_000_000),
            Call::Baseband(Chain::Rx, Both, 1_000_000),
            Call::Baseband(Chain::Tx, Both, 1_000_000),
        ],
    );
}
