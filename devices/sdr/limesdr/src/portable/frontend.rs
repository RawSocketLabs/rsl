//! What the LimeSDR-mini gives libusdr's LMS7002M control code: its automatic bands
//! (`limesdr_init`'s `cfg_auto_rx`, `cfg_auto_tx`), its antenna switches
//! (`_limesdr_antenna_port_switch`) and its `LimeLight` sample order
//! (`_limesdr_lml_portcfg`).

use lms7002m::ctrl::{Direction, Frontend, RxAntenna, StreamFlags, TxAntenna};
use lms7002m::{LmlMap, LmlSample, Mac, RxPath, TxPath};

use super::fpga::{RF_SWITCHES, RX_SWITCH_SHIFT, TX_SWITCH_SHIFT};
use super::modify::Modify;

/// The receive bands: the wide-band LNA ("LNAW", switch 2) below 1.7 GHz, the high band
/// ("LNAH", 1) below 4 GHz, the low band ("LNAL", 0) above; in RF loopback the switches
/// are 1, 2 and 0. libusdr's band search never reads the last band's 4.1 GHz edge.
pub(crate) const RX_BANDS: [RxAntenna; 3] = [
    RxAntenna {
        below_hz: 1_700_000_000,
        path: RxPath::LnaW,
        switch: 2,
        loopback_switch: 1,
    },
    RxAntenna {
        below_hz: 4_000_000_000,
        path: RxPath::LnaH,
        switch: 1,
        loopback_switch: 2,
    },
    RxAntenna {
        below_hz: 4_100_000_000,
        path: RxPath::LnaL,
        switch: 0,
        loopback_switch: 0,
    },
];

/// The transmit bands: band 2 ("W"/"B2", switch 2) below 2 GHz, band 1 ("H"/"B1", 1)
/// above; their switch positions swap in RF loopback.
pub(crate) const TX_BANDS: [TxAntenna; 2] = [
    TxAntenna {
        below_hz: 2_000_000_000,
        path: TxPath::Band2,
        switch: 2,
        loopback_switch: 1,
        names: ["W", "B2"],
    },
    TxAntenna {
        below_hz: 4_000_000_000,
        path: TxPath::Band1,
        switch: 1,
        loopback_switch: 2,
        names: ["H", "B1"],
    },
];

/// The LimeSDR-mini as the control code's board.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LimeFrontend;

impl Frontend for LimeFrontend {
    type Switch = Modify;

    /// The direction's two bits of [`RF_SWITCHES`], read, replaced and written back.
    /// libusdr shifts the position unmasked; every band's position fits.
    fn antenna_switch(&self, direction: Direction, position: u8) -> Modify {
        let shift = match direction {
            Direction::Rx => RX_SWITCH_SHIFT,
            Direction::Tx => TX_SWITCH_SHIFT,
        };
        Modify::new(RF_SWITCHES, 0b11 << shift, u16::from(position) << shift)
    }

    /// Always channel A then B, I before Q: libusdr's table also has the IQ swap, but
    /// returns its first entry whatever the stream asks.
    fn lml_map(&self, _rx: bool, _channels: Mac, _flags: StreamFlags) -> LmlMap {
        use LmlSample::{AI, AQ, BI, BQ};
        [AI, AQ, BI, BQ]
    }
}
