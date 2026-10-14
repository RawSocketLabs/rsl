//! What the XSDR gives libusdr's LMS7002M control code: its antenna switches
//! (`_xsdr_antenna_port_switch`) and `LimeLight` sample orders (`lms7nfe_get_lml_portcfg`),
//! and its automatic bands (`_xsdr_init_revx`).

use lms7002m::ctrl::{Direction, Frontend, RxAntenna, StreamFlags, TxAntenna};
use lms7002m::{LmlMap, LmlSample, Mac, RxPath, TxPath};

use super::fpga::Gpo;
use super::step::Step;

/// The XSDR's receive bands (`cfg_auto_rx`): the wide-band LNA below 2.2 GHz, the high
/// band below 4 GHz, and the low band (named "EXT") past the last edge.
pub(crate) const RX_BANDS: [RxAntenna; 3] = [
    RxAntenna {
        below_hz: 2_200_000_000,
        path: RxPath::LnaW,
        switch: 0,
    },
    RxAntenna {
        below_hz: 4_000_000_000,
        path: RxPath::LnaH,
        switch: 1,
    },
    RxAntenna {
        below_hz: 4_000_000_000,
        path: RxPath::LnaL,
        switch: 1,
    },
];

/// The XSDR's transmit bands (`cfg_auto_tx`, without the `USDR_TX_W`/`USDR_TX_H`
/// overrides the oracle forbids): band 1 below 2.2 GHz, band 2 above.
pub(crate) const TX_BANDS: [TxAntenna; 2] = [
    TxAntenna {
        below_hz: 2_200_000_000,
        path: TxPath::Band1,
        switch: 1,
    },
    TxAntenna {
        below_hz: 4_000_000_000,
        path: TxPath::Band2,
        switch: 0,
    },
];

/// The XSDR as the control code's board.
#[derive(Clone, Copy, Debug)]
pub(crate) struct XsdrFrontend;

impl Frontend for XsdrFrontend {
    type Switch = Step;

    fn antenna_switch(&self, direction: Direction, position: u8) -> Step {
        let bank = match direction {
            Direction::Rx => Gpo::RxSwitch,
            Direction::Tx => Gpo::TxSwitch,
        };
        Step::fpga(bank.write(position))
    }

    fn lml_map(&self, _rx: bool, channels: Mac, flags: StreamFlags) -> LmlMap {
        use LmlSample::{AI, AQ, BI, BQ};
        let swap_ab = (channels == Mac::Both && flags.swap_ab) || channels == Mac::B;
        match (swap_ab, flags.swap_iq) {
            (false, false) => [AI, AQ, BI, BQ],
            (false, true) => [AQ, AI, BQ, BI],
            (true, false) => [BI, BQ, AI, AQ],
            (true, true) => [BQ, BI, AQ, AI],
        }
    }
}
