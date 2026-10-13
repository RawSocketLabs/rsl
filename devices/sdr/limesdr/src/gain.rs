//! The gains libusdr offers on this family: its `/dm/sdr/0/{rx,tx}/gain*` paths.

use lms7002m::ctrl;

/// A gain control, one per libusdr device-manager path (`u3_limesdr.c`). libusdr's LMS7
/// family also has a TX amplifier gain; this family offers none, so neither does this.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gain {
    /// The LNA, 0–30 dB (`/dm/sdr/0/rx/gain` and `/dm/sdr/0/rx/gain/lna`).
    RxLna,

    /// The TIA, as an attenuation: −12, −3 or 0 dB (`/dm/sdr/0/rx/gain/vga`).
    RxTia,

    /// The PGA, 0–31 dB (`/dm/sdr/0/rx/gain/pga`).
    RxPga,

    /// The RF loopback, as an attenuation (`/dm/sdr/0/rx/gain/lb`).
    RxLoopback,

    /// The TX pad (`/dm/sdr/0/tx/gain`); positive values are taken as 0.
    TxPad,

    /// The TX pad's loopback (`/dm/sdr/0/tx/gain/lb`); positive values are taken as 0.
    TxLoopback,
}

impl Gain {
    /// The control code's gain this path sets (`lms7002m_set_gain`'s type).
    pub(crate) const fn control(self) -> ctrl::Gain {
        match self {
            Self::RxLna => ctrl::Gain::RxLna,
            Self::RxTia => ctrl::Gain::RxTia,
            Self::RxPga => ctrl::Gain::RxPga,
            Self::RxLoopback => ctrl::Gain::RxLoopback,
            Self::TxPad => ctrl::Gain::TxPad,
            Self::TxLoopback => ctrl::Gain::TxLoopback,
        }
    }
}
