//! What the XSDR and XTRX give libusdr's LMS7002M control code: their antenna switches
//! (`_xsdr_antenna_port_switch`) and `LimeLight` sample orders (`lms7nfe_get_lml_portcfg`),
//! and their automatic bands (`_xsdr_init_revx`, `_xsdr_init_revo`).

use lms7002m::ctrl::{Direction, Frontend, RxAntenna, StreamFlags, TxAntenna};
use lms7002m::{LmlMap, LmlSample, Mac, RxPath, TxPath};

use super::fpga::Gpo;
use super::step::Step;

/// The XSDR's receive bands (`cfg_auto_rx`): the wide-band LNA below 2.2 GHz, the high
/// band below 4 GHz, and the low band (named "EXT") past the last edge; in RF loopback
/// the wide band's switch moves to 1 and the others' to 0.
pub(crate) const RX_BANDS: [RxAntenna; 3] = [
    RxAntenna {
        below_hz: 2_200_000_000,
        path: RxPath::LnaW,
        switch: 0,
        loopback_switch: 1,
    },
    RxAntenna {
        below_hz: 4_000_000_000,
        path: RxPath::LnaH,
        switch: 1,
        loopback_switch: 0,
    },
    RxAntenna {
        below_hz: 4_000_000_000,
        path: RxPath::LnaL,
        switch: 1,
        loopback_switch: 0,
    },
];

/// The XSDR's transmit bands (`cfg_auto_tx`, without the `USDR_TX_W`/`USDR_TX_H`
/// overrides the oracle forbids): band 1 ("W", "B1") below 2.2 GHz, band 2 ("H", "B2")
/// above; their switch positions swap in RF loopback.
pub(crate) const TX_BANDS: [TxAntenna; 2] = [
    TxAntenna {
        below_hz: 2_200_000_000,
        path: TxPath::Band1,
        switch: 1,
        loopback_switch: 0,
        names: ["W", "B1"],
    },
    TxAntenna {
        below_hz: 4_000_000_000,
        path: TxPath::Band2,
        switch: 0,
        loopback_switch: 1,
        names: ["H", "B2"],
    },
];

/// The XTRX's receive bands (`_xsdr_init_revo`'s `cfg_auto_rx`): the low-band LNA below
/// 1.1 GHz on switch 1, the wide band below 2.7 GHz on 0, the high band below 4 GHz on 2;
/// every band's loopback switch is 3.
pub(crate) const XTRX_RX_BANDS: [RxAntenna; 3] = [
    RxAntenna {
        below_hz: 1_100_000_000,
        path: RxPath::LnaL,
        switch: 1,
        loopback_switch: 3,
    },
    RxAntenna {
        below_hz: 2_700_000_000,
        path: RxPath::LnaW,
        switch: 0,
        loopback_switch: 3,
    },
    RxAntenna {
        below_hz: 4_000_000_000,
        path: RxPath::LnaH,
        switch: 2,
        loopback_switch: 3,
    },
];

/// The XTRX's transmit bands (`_xsdr_init_revo`'s `cfg_auto_tx`): band 2 ("W", "B2") below
/// 2.2 GHz on switch 0, band 1 ("H", "B1") below 4 GHz on 1; their loopback switches swap.
pub(crate) const XTRX_TX_BANDS: [TxAntenna; 2] = [
    TxAntenna {
        below_hz: 2_200_000_000,
        path: TxPath::Band2,
        switch: 0,
        loopback_switch: 1,
        names: ["W", "B2"],
    },
    TxAntenna {
        below_hz: 4_000_000_000,
        path: TxPath::Band1,
        switch: 1,
        loopback_switch: 0,
        names: ["H", "B1"],
    },
];

/// The XSDR (and the XTRX, whose hooks are the same) as the control code's board.
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
