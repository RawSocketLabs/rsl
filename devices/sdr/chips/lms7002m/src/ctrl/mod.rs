//! The LMS7002M control code boards share: libusdr's `device/m2_lm7_1/lms7002m_ctrl.c`
//! (`lms7002_dev_t` and its operations), which both its LMS7 family and its `LimeSDR`
//! call.
//!
//! [`Control`] is the state; each operation takes it by value and returns an
//! [`Operation`], a hand-written sans-I/O machine that owns the state while it runs and
//! gives it back ([`Operation::control`]) with libusdr's updates, also after a failure.
//! What only a board knows (its antenna switches and `LimeLight` sample order) comes from
//! its [`Frontend`].
//!
//! Ported: the sample rate (`lms7002m_samplerate`), gains (`lms7002m_set_gain`),
//! baseband bandwidths (`lms7002m_bb_set_badwidth`), `lms7002m_streaming_up` for RX and
//! for RX with TX, the `LimeLight` RX mode (`lms7002m_set_lmlrx_mode`), LO tuning
//! (`lms7002m_fe_set_freq`, FDD and TDD), the RX and TX paths with their RF loopback
//! (`lms7002m_rfe_set_path`, `_tfe_set_path`, `_fe_set_lna`) over the board's band
//! tables, and the baseband NCOs (`lms7002m_bb_set_freq`). Not ported: streaming up TX
//! alone and the calibration helpers (`_set_corr_param`, `_set_tx_testsig`,
//! `_update_bandwidth`).

mod calls;
mod op;
mod part;
mod path;
mod rate;

pub use calls::{ChipCall, ChipCalls};
pub use op::{ControlError, Operation};
pub use part::NoBoard;
pub use rate::{RateFlags, RateRequest};

pub use crate::chip::Synthesizer;

use core::num::NonZeroU32;

use hal::Sequence;

use crate::chip::{LmlConfig, LmlMap, Lms7002m, LpfParams, RbbPath, TbbPath};
use crate::regs::{LmlSample, Mac, RxPath};
use crate::{RxGainStage, TxGainStage, TxPath};
use calls::Calls;

/// A direction: libusdr's `RFIC_LMS7_RX` and `RFIC_LMS7_TX`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Receive.
    Rx,

    /// Transmit.
    Tx,
}

/// One receive band: below `below_hz`, the LNA `path` with the board's antenna switch at
/// `switch`, or at `loopback_switch` while the RF loopback is on (`freq_auto_band_map_t`:
/// `stop_freq`, `band`, `sw`, `swlb`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxAntenna {
    /// The band's upper edge in hertz.
    pub below_hz: u64,

    /// The LNA.
    pub path: RxPath,

    /// The board's switch position.
    pub switch: u8,

    /// The board's switch position while the RF loopback is on.
    pub loopback_switch: u8,
}

/// One transmit band, as [`RxAntenna`] for the TX band outputs; `names` are libusdr's
/// `name0` and `name1`, which [`TxRoute::W`] and [`TxRoute::H`] look up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxAntenna {
    /// The band's upper edge in hertz.
    pub below_hz: u64,

    /// The band output.
    pub path: TxPath,

    /// The board's switch position.
    pub switch: u8,

    /// The board's switch position while the RF loopback is on.
    pub loopback_switch: u8,

    /// The band's names, compared case-insensitively.
    pub names: [&'static str; 2],
}

/// The receive path: libusdr's `rfic_lms7_rf_path_t` RX values. `XSDR_RX_ADC_EXT`, which
/// libusdr refuses with `-EINVAL` before any state change or access, has no counterpart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RxRoute {
    /// The band follows the RX LO (`XSDR_RX_AUTO`).
    Auto,

    /// The low-band LNA (`XSDR_RX_L`).
    LnaL,

    /// The high-band LNA (`XSDR_RX_H`).
    LnaH,

    /// The wide-band LNA (`XSDR_RX_W`).
    LnaW,

    /// The low-band LNA fed back from TX band 2 (`XSDR_RX_L_TX_B2_LB`).
    LoopbackL,

    /// The wide-band LNA fed back from TX band 1 (`XSDR_RX_W_TX_B1_LB`).
    LoopbackW,

    /// The high-band LNA fed back from TX band 1 (`XSDR_RX_H_TX_B1_LB`).
    LoopbackH,
}

/// The transmit path: libusdr's `rfic_lms7_rf_path_t` TX values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxRoute {
    /// The band follows the TX LO (`XSDR_TX_AUTO`).
    Auto,

    /// The band whose output is band 1 (`XSDR_TX_B1`).
    Band1,

    /// The band whose output is band 2 (`XSDR_TX_B2`).
    Band2,

    /// The band named `W` (`XSDR_TX_W`).
    W,

    /// The band named `H` (`XSDR_TX_H`).
    H,
}

/// A path for either direction, as `lms7002m_fe_set_lna` takes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RfPath {
    /// A receive path.
    Rx(RxRoute),

    /// A transmit path.
    Tx(TxRoute),
}

/// The index of the band for `frequency` (`get_antenna_cfg_by_freq`): the first whose edge
/// is above it, else the last.
fn band_for(frequency: u32, edges: impl ExactSizeIterator<Item = u64>) -> usize {
    let last = edges.len() - 1;
    edges
        .take(last)
        .position(|edge| u64::from(frequency) < edge)
        .unwrap_or(last)
}

/// What the control code needs from the board: libusdr's `on_ant_port_sw` and
/// `on_get_lml_portcfg` callbacks.
pub trait Frontend: Copy {
    /// The board's antenna switch, as a sequence.
    type Switch: Sequence + Copy;

    /// Sets the `direction` antenna switch to `position` (a band's `switch`).
    fn antenna_switch(&self, direction: Direction, position: u8) -> Self::Switch;

    /// The `LimeLight` sample order for a stream on `channels` with `flags`.
    fn lml_map(&self, rx: bool, channels: Mac, flags: StreamFlags) -> LmlMap;
}

/// The stream flags the ported control code reads: libusdr's `RFIC_SWAP_AB`,
/// `RFIC_SWAP_IQ` and `RFIC_NO_DC_COMP`. Its `RFIC_LFSR` and `RFIC_DIGITAL_LB`, which
/// reconfigure the `LimeLight` interface at the end of streaming up, are not ported.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StreamFlags {
    /// Channel B's samples where channel A's go (`RFIC_SWAP_AB`); with SISO, stream B.
    pub swap_ab: bool,

    /// Q where I goes (`RFIC_SWAP_IQ`).
    pub swap_iq: bool,

    /// The RX DC corrector bypassed (`RFIC_NO_DC_COMP`).
    pub no_dc_correction: bool,
}

/// The board of the operations that call no hook. Only this crate builds one, so an
/// operation that needs a board's hooks cannot be given it.
#[derive(Clone, Copy, Debug)]
pub struct NoFrontend(());

impl Frontend for NoFrontend {
    type Switch = NoBoard;

    fn antenna_switch(&self, _direction: Direction, _position: u8) -> NoBoard {
        unreachable!("invariant: no operation given NoFrontend switches an antenna")
    }

    fn lml_map(&self, _rx: bool, _channels: Mac, _flags: StreamFlags) -> LmlMap {
        unreachable!("invariant: no operation given NoFrontend maps a stream")
    }
}

/// A gain control: libusdr's `rfic_lms7_gain_types` without the automatic ones, which
/// libusdr maps to [`Self::RxLna`] and [`Self::TxPad`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gain {
    /// The LNA, 0–30 dB (`RFIC_LMS7_RX_LNA_GAIN`).
    RxLna,

    /// The TIA, as an attenuation: −12, −3 or 0 dB (`RFIC_LMS7_RX_TIA_GAIN`).
    RxTia,

    /// The PGA, 0–31 dB (`RFIC_LMS7_RX_PGA_GAIN`).
    RxPga,

    /// The RF loopback, as an attenuation (`RFIC_LMS7_RX_LB_GAIN`).
    RxLoopback,

    /// The TX pad (`RFIC_LMS7_TX_PAD_GAIN`); positive values are taken as 0.
    TxPad,

    /// The TX pad's loopback (`RFIC_LMS7_TX_LB_GAIN`); positive values are taken as 0.
    TxLoopback,

    /// The TX amplifier's correction, −62 to 63 codes (`RFIC_LMS7_TX_PGA_GAIN`).
    TxPga,
}

/// The receive PGA's input for a bandwidth, and whether the low-band filter serves it
/// (`lms7002m_rbb_bandwidth`): over 200 MHz none, over 20 MHz the high band.
fn rbb_filter(bandwidth: u32) -> RbbPath {
    match bandwidth {
        200_000_001.. => RbbPath::Bypass,
        20_000_001.. => RbbPath::HighBand,
        _ => RbbPath::LowBand,
    }
}

/// The transmit filter for a bandwidth (`lms7002m_tbb_bandwidth`).
fn tbb_filter(bandwidth: u32) -> TbbPath {
    match bandwidth {
        200_000_001.. => TbbPath::Bypass,
        20_000_001.. => TbbPath::HighBand,
        _ => TbbPath::Ladder,
    }
}

/// `_ulog`: the decimation order of a divider (1 → 0 … 16 → 4; anything else 5).
const fn order(divider: u32) -> u32 {
    match divider {
        1 => 0,
        2 => 1,
        4 => 2,
        8 => 3,
        16 => 4,
        _ => 5,
    }
}

/// The clocks a sample rate sets (`lms7002_dev_t`: `cgen_clk`, `*cgen_div`, `*tsp_div`,
/// `rx_dsp_decim`, `tx_dsp_inter`): CGEN → converter divider → processor divider → the
/// board's own decimation or interpolation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clocks {
    /// The CGEN frequency in hertz; 0 until a rate is set.
    pub cgen: u32,

    /// The ADC's divider from CGEN (`rxcgen_div`).
    pub rx_converter_div: u32,

    /// The DAC's divider from CGEN (`txcgen_div`).
    pub tx_converter_div: u32,

    /// The RX processor's decimation (`rxtsp_div`).
    pub rx_tsp_div: u32,

    /// The TX processor's interpolation (`txtsp_div`).
    pub tx_tsp_div: u32,

    /// The board's RX decimation after the chip (`rx_dsp_decim`).
    pub rx_decimation: u32,

    /// The board's TX interpolation before the chip (`tx_dsp_inter`).
    pub tx_interpolation: u32,
}

/// The control code's state: libusdr's `lms7002_dev_t` for the ported operations. Its
/// zero is `xsdr_ctor`'s `memset` and `lms7002m_init`.
#[derive(Clone, Copy, Debug)]
pub struct Control {
    /// The chip's cache.
    chip: Lms7002m,

    /// The reference clock in hertz (`fref`).
    reference: NonZeroU32,

    /// Each channel's receive path runs (`rx_run`).
    rx_running: [bool; 2],

    /// Each channel's transmit path runs (`tx_run`).
    tx_running: [bool; 2],

    /// The clocks.
    clocks: Clocks,

    /// The `LimeLight` configuration last written (`lml_mode`).
    lml: LmlConfig,

    /// A bandwidth set per receive channel (`rx_bw`); `None` follows the sample rate.
    rx_bandwidth: [Option<u32>; 2],

    /// As [`Self::rx_bandwidth`], for transmit (`tx_bw`).
    tx_bandwidth: [Option<u32>; 2],

    /// The RX LO in hertz (`rx_lo`).
    rx_lo: u32,

    /// The TX LO in hertz (`tx_lo`).
    tx_lo: u32,

    /// The board's receive bands (`cfg_auto_rx`).
    rx_bands: [RxAntenna; 3],

    /// The board's transmit bands (`cfg_auto_tx`).
    tx_bands: [TxAntenna; 2],

    /// The channels and flags the receive direction last streamed up with
    /// (`lml_rx_chs`, `lml_rx_flags`).
    rx_stream: (Mac, StreamFlags),

    /// As [`Self::rx_stream`], for transmit (`lml_tx_chs`, `lml_tx_flags`).
    tx_stream: (Mac, StreamFlags),

    /// The receive stream's sample order (`map_rx`).
    rx_map: LmlMap,

    /// The transmit stream's sample order (`map_tx`).
    tx_map: LmlMap,

    /// The receive stream is SISO (`rx_siso`).
    rx_siso: bool,

    /// The transmit stream is SISO (`tx_siso`).
    tx_siso: bool,

    /// The receive path (`rx_rfic_path`).
    rx_route: RxRoute,

    /// The transmit path (`tx_rfic_path`).
    tx_route: TxRoute,

    /// The receive band in use, an index into [`Self::rx_bands`] (`rx_cfg_path`).
    rx_band: usize,

    /// The transmit band in use (`tx_cfg_path`).
    tx_band: usize,

    /// The RF loopback is on (`rx_lna_lb_active`).
    loopback: bool,

    /// Each channel's TX pad loss as `lms7002m_set_gain` stores it: the reported gain over
    /// −10, truncated to a byte (`tx_loss`).
    tx_loss: [u8; 2],

    /// Each channel's RX NCO word, once set (`rx_dsp`).
    rx_nco: [Option<i32>; 2],

    /// Each channel's TX NCO word (`tx_dsp`).
    tx_nco: [Option<i32>; 2],
}

impl Control {
    /// The state `xsdr_ctor` and `lms7002m_init` leave: nothing running, no rate, both
    /// paths automatic over the board's bands.
    #[must_use]
    pub const fn new(
        chip: Lms7002m,
        reference: NonZeroU32,
        rx_bands: [RxAntenna; 3],
        tx_bands: [TxAntenna; 2],
    ) -> Self {
        Self {
            chip,
            reference,
            rx_running: [false; 2],
            tx_running: [false; 2],
            clocks: Clocks {
                cgen: 0,
                rx_converter_div: 0,
                tx_converter_div: 0,
                rx_tsp_div: 0,
                tx_tsp_div: 0,
                rx_decimation: 0,
                tx_interpolation: 0,
            },
            lml: LmlConfig {
                rx_siso_ddr: false,
                tx_siso_ddr: false,
                high_drive: false,
                rx_fclk_read: false,
                rx_lfsr: false,
                rx_tx_loopback: false,
                rx_port1: false,
                rx_div: 0,
                tx_div: 0,
                tx_tsp_delay: 0,
                tx_lml_delay: 0,
            },
            rx_bandwidth: [None; 2],
            tx_bandwidth: [None; 2],
            rx_lo: 0,
            tx_lo: 0,
            rx_bands,
            tx_bands,
            rx_stream: (
                Mac::None,
                StreamFlags {
                    swap_ab: false,
                    swap_iq: false,
                    no_dc_correction: false,
                },
            ),
            tx_stream: (
                Mac::None,
                StreamFlags {
                    swap_ab: false,
                    swap_iq: false,
                    no_dc_correction: false,
                },
            ),
            rx_map: [LmlSample::AI; 4],
            tx_map: [LmlSample::AI; 4],
            rx_siso: false,
            tx_siso: false,
            rx_route: RxRoute::Auto,
            tx_route: TxRoute::Auto,
            rx_band: 0,
            tx_band: 0,
            loopback: false,
            tx_loss: [0; 2],
            rx_nco: [None; 2],
            tx_nco: [None; 2],
        }
    }

    /// The chip's cache.
    #[must_use]
    pub const fn chip(&self) -> &Lms7002m {
        &self.chip
    }

    /// The chip's cache, for the board's own chip operations.
    pub const fn chip_mut(&mut self) -> &mut Lms7002m {
        &mut self.chip
    }

    /// Which receive channels run.
    #[must_use]
    pub const fn rx_running(&self) -> [bool; 2] {
        self.rx_running
    }

    /// Which transmit channels run.
    #[must_use]
    pub const fn tx_running(&self) -> [bool; 2] {
        self.tx_running
    }

    /// The clocks the last sample rate set.
    #[must_use]
    pub const fn clocks(&self) -> Clocks {
        self.clocks
    }

    /// The `LimeLight` configuration last written.
    #[must_use]
    pub const fn lml(&self) -> LmlConfig {
        self.lml
    }

    /// The RX and TX LOs in hertz.
    #[must_use]
    pub const fn lo(&self) -> (u32, u32) {
        (self.rx_lo, self.tx_lo)
    }

    /// The reference clock in hertz (`fref`).
    #[must_use]
    pub const fn reference(&self) -> NonZeroU32 {
        self.reference
    }

    /// The receive and transmit paths last selected (`rx_rfic_path`, `tx_rfic_path`).
    #[must_use]
    pub const fn routes(&self) -> (RxRoute, TxRoute) {
        (self.rx_route, self.tx_route)
    }

    /// Each channel's TX pad loss as stored (`tx_loss`).
    #[must_use]
    pub const fn tx_loss(&self) -> [u8; 2] {
        self.tx_loss
    }

    /// Each channel's `direction` NCO word, once set (`rx_dsp`, `tx_dsp`).
    #[must_use]
    pub const fn nco_words(&self, direction: Direction) -> [Option<i32>; 2] {
        match direction {
            Direction::Rx => self.rx_nco,
            Direction::Tx => self.tx_nco,
        }
    }

    /// Sets a gain on both channels (`lms7002m_set_gain` with `LMS7_CH_AB`): both banks
    /// selected, then the stage written. Returns the operation and the gain libusdr
    /// reports as reached, in dB.
    #[must_use]
    pub fn set_gain(mut self, gain: Gain, value: i32) -> (Operation, i32) {
        let mut calls = Calls::new();
        calls.push(ChipCall::MacSet(Mac::Both));
        let mut receive = |stage: RxGainStage, gain_x10: i32| {
            calls.push(ChipCall::RfeGain(stage, gain_x10));
            stage.reached(gain_x10)
        };
        let actual = match gain {
            Gain::RxLna => {
                (receive(RxGainStage::Lna, value.wrapping_sub(30).wrapping_mul(10)) + 300) / 10
            }
            Gain::RxTia => receive(RxGainStage::Tia, value.wrapping_mul(10)) / 10,
            Gain::RxLoopback => receive(RxGainStage::Loopback, value.wrapping_mul(10)) / 10,
            Gain::RxPga => {
                let value = value.clamp(0, 31);
                calls.push(ChipCall::RbbPga(value * 10));
                value
            }
            Gain::TxPad | Gain::TxLoopback => {
                let value = value.min(0);
                let stage = if gain == Gain::TxPad {
                    TxGainStage::Pad
                } else {
                    TxGainStage::Loopback
                };
                // libusdr passes -10 * gain where the pad expects a gain: reproduced.
                let gain_x10 = value.wrapping_mul(-10);
                calls.push(ChipCall::TrfGain(stage, gain_x10));
                if let Some(reached) = stage.reached(gain_x10) {
                    // `tx_loss[i] = aret / -10`, kept in a byte.
                    let [loss, ..] = (reached / -10).to_le_bytes();
                    self.tx_loss = [loss; 2];
                }
                value
            }
            Gain::TxPga => {
                let [low, ..] = value.clamp(-62, 63).to_le_bytes();
                calls.push(ChipCall::TbbGain(i8::from_le_bytes([low])));
                0
            }
        };
        (Operation::writes(&self, &calls), actual)
    }

    /// Sets a baseband bandwidth on both channels (`lms7002m_bb_set_badwidth` with
    /// `LMS7_CH_AB`): channel by channel, its bank selected, then the filter path and
    /// codes for `hertz`. 0 clears the setting, so the sample rate decides again, and
    /// writes nothing.
    #[must_use]
    pub fn set_bandwidth(mut self, direction: Direction, hertz: u32) -> Operation {
        let mut calls = Calls::new();
        for (index, bank) in [Mac::A, Mac::B].into_iter().enumerate() {
            let setting = if hertz == 0 { None } else { Some(hertz) };
            match direction {
                Direction::Rx => self.rx_bandwidth[index] = setting,
                Direction::Tx => self.tx_bandwidth[index] = setting,
            }
            let Some(hertz) = NonZeroU32::new(hertz) else {
                continue;
            };
            calls.push(ChipCall::MacSet(bank));
            push_bandwidth(&mut calls, direction, hertz);
        }
        Operation::writes(&self, &calls)
    }

    /// Refreshes the running channels' baseband filters in `direction` for `rate`
    /// (`lms7002m_update_bandwidth` forcing the update; libusdr's distance to the farthest
    /// NCO, never set, adds nothing): channel by channel, its bank selected, then the
    /// filter for its own bandwidth if one is set, else for `rate`. As in libusdr, a
    /// channel after one with its own bandwidth takes that bandwidth too. A `rate` of 0
    /// writes nothing.
    #[must_use]
    pub fn update_bandwidth(self, direction: Direction, rate: u32) -> Operation {
        let mut calls = Calls::new();
        let (running, set) = match direction {
            Direction::Rx => (self.rx_running, self.rx_bandwidth),
            Direction::Tx => (self.tx_running, self.tx_bandwidth),
        };
        let mut hertz = NonZeroU32::new(rate);
        for ((bank, running), set) in [Mac::A, Mac::B].into_iter().zip(running).zip(set) {
            let Some(rate_hertz) = hertz.filter(|_| running) else {
                continue;
            };
            let chosen = set.and_then(NonZeroU32::new).unwrap_or(rate_hertz);
            hertz = Some(chosen);
            calls.push(ChipCall::MacSet(bank));
            push_bandwidth(&mut calls, direction, chosen);
        }
        Operation::writes(&self, &calls)
    }

    /// Delays the TX processors' clocks by one code more (`lml_mode.txtspdelay++`, then
    /// `lms7002m_limelight_upd_delay`), as the XSDR's TX quality check does between
    /// attempts. The code wraps at eight bits; the register takes its low two.
    #[must_use]
    pub fn delay_tx_tsp(mut self) -> Operation {
        self.lml.tx_tsp_delay = self.lml.tx_tsp_delay.wrapping_add(1);
        let mut calls = Calls::new();
        calls.push(ChipCall::LimelightDelays(self.lml));
        Operation::writes(&self, &calls)
    }

    /// Switches the receive FIFO between the processor, the LFSR pattern and the TX
    /// loopback (`lms7002m_set_lmlrx_mode`): `lfsr` wins over `loopback`.
    #[must_use]
    pub fn set_lml_rx_mode(mut self, lfsr: bool, loopback: bool) -> Operation {
        self.lml.rx_lfsr = lfsr;
        self.lml.rx_tx_loopback = loopback;
        let mut calls = Calls::new();
        calls.push(ChipCall::LimelightSwitchRxMode(self.lml));
        Operation::writes(&self, &calls)
    }
}

/// `lms7002m_rbb_bandwidth` or `lms7002m_tbb_bandwidth` without loopback: the filter
/// path for `hertz`, then its codes, on the selected banks.
#[must_use]
pub fn bandwidth_calls(direction: Direction, hertz: NonZeroU32) -> [ChipCall; 2] {
    match direction {
        Direction::Rx => {
            let path = rbb_filter(hertz.get());
            let params = LpfParams::rbb_default(hertz, path == RbbPath::LowBand);
            [
                ChipCall::RbbPath(path, crate::RbbMode::Normal),
                ChipCall::RbbLpf(params),
            ]
        }
        Direction::Tx => {
            let path = tbb_filter(hertz.get());
            let params = LpfParams::tbb_default(hertz.get(), path == TbbPath::Ladder);
            [
                ChipCall::TbbPath(path, crate::TbbMode::Normal),
                ChipCall::TbbLpf(params),
            ]
        }
    }
}

/// [`bandwidth_calls`], queued.
fn push_bandwidth(calls: &mut Calls, direction: Direction, hertz: NonZeroU32) {
    for call in bandwidth_calls(direction, hertz) {
        calls.push(call);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_band_takes_everything_above_the_others() {
        let edges = [2_200_000_000_u64, 4_000_000_000, 4_000_000_000];
        assert_eq!(band_for(1_000_000_000, edges.into_iter()), 0);
        assert_eq!(band_for(2_200_000_000, edges.into_iter()), 1);
        assert_eq!(band_for(4_100_000_000, edges.into_iter()), 2);
    }

    #[test]
    fn the_decimation_order_of_an_unknown_divider_is_five() {
        assert_eq!(order(16), 4);
        assert_eq!(order(32), 5);
        assert_eq!(order(3), 5);
    }
}
