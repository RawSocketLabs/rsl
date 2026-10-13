//! The low-pass filters: the TX filter (datasheet `TxLPF`, registers 0x30-0x36) and the RX
//! filter (`RxLPF`, 0x50-0x56). The two blocks share a map 0x20 apart; the driver uses
//! their bandwidth, bypass and control registers (0x34-0x36 and 0x54-0x56), each indexed
//! by [`Lpf`].
//!
//! The RX filter sits between RXVGA1 and RXVGA2 and limits how much spectrum reaches the
//! ADC; the TX filter sits between the DAC and TXVGA1 and limits what the DAC's images
//! leave in the output. Each has to suit the sample rate. libusdr picks a bandwidth code and
//! an RC calibration value from one fixed table for both ([`for_bandwidth`]); above its last
//! entry it bypasses the filter.

use bnb::{BitEnum, bitfield, u3, u4, u6};

use super::super::register::IndexedRegister;
use super::spi::BlockReg;

/// Filter register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(super) enum Reg {
    /// TX LPF `BW`; see [`Bandwidth`].
    TxBandwidth = 0x34,

    /// TX LPF `DACBP`; see [`Bypass`].
    TxBypass = 0x35,

    /// TX LPF `CTRL`; see [`Control`].
    TxControl = 0x36,

    /// RX LPF `BW`.
    RxBandwidth = 0x54,

    /// RX LPF `DACBP`.
    RxBypass = 0x55,

    /// RX LPF `CTRL`.
    RxControl = 0x56,
}

impl BlockReg for Reg {}

/// The filter's bandwidth, by its YAML code.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u4)]
pub(super) enum LpfBandwidth {
    /// 14 MHz.
    Mhz14,

    /// 10 MHz.
    Mhz10,

    /// 7 MHz.
    Mhz7,

    /// 6 MHz.
    Mhz6,

    /// 5 MHz.
    Mhz5,

    /// 4.375 MHz.
    Mhz4_375,

    /// 3.5 MHz.
    Mhz3_5,

    /// 3 MHz.
    Mhz3,

    /// 2.75 MHz.
    Mhz2_75,

    /// 2.5 MHz.
    Mhz2_5,

    /// 1.92 MHz.
    Mhz1_92,

    /// 1.5 MHz.
    Mhz1_5,

    /// 1.375 MHz.
    Mhz1_375,

    /// 1.25 MHz.
    Mhz1_25,

    /// 0.875 MHz.
    Mhz0_875,

    /// 0.75 MHz.
    Mhz0_75,
}

/// The filter's bandwidth and power. Datasheet `BW`, registers 0x34 (TX) and 0x54 (RX).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Bandwidth {
    /// Filter bandwidth. `LPF`, bits 5:2.
    #[bits(2..=5)]
    bandwidth: LpfBandwidth,

    /// Power the LPF modules. `EN`, bit 1.
    #[bits(1..=1)]
    enabled: bool,

    /// Take control signals from the test-mode registers instead of decoding them.
    /// `DECODE`, bit 0; reset 0.
    #[bits(0..=0)]
    test_mode_controls: bool,
}
impl IndexedRegister for Bandwidth {
    type Map = Reg;
    type Index = Lpf;

    fn addr(lpf: Lpf) -> Reg {
        match lpf {
            Lpf::Tx => Reg::TxBandwidth,
            Lpf::Rx => Reg::RxBandwidth,
        }
    }
}

/// Which filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Lpf {
    /// The TX filter.
    Tx,

    /// The RX filter.
    Rx,
}

/// The filter bypass and the DC-offset DAC's resistor calibration. Datasheet `DACBP`,
/// registers 0x35 (TX) and 0x55 (RX).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Bypass {
    /// Route around the filter. `BYP`, bit 6.
    #[bits(6..=6)]
    bypassed: bool,

    /// Resistor calibration of the DC-offset cancellation DAC; the YAML gives no meaning
    /// per code. `DACCAL`, bits 5:0.
    #[bits(0..=5)]
    dc_dac_calibration: u6,
}
impl IndexedRegister for Bypass {
    type Map = Reg;
    type Index = Lpf;

    fn addr(lpf: Lpf) -> Reg {
        match lpf {
            Lpf::Tx => Reg::TxBypass,
            Lpf::Rx => Reg::RxBypass,
        }
    }
}

/// RC calibration and power-downs. Datasheet `CTRL`, registers 0x36 (TX) and 0x56 (RX).
///
/// The TX filter's register also has `TX_DACBUF_PD` (bit 7: power the TX DAC's buffers
/// down; reset 0), which the RX filter's lacks. libusdr writes it 0 with every bandwidth,
/// as this type, which leaves the bit out, does.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Control {
    /// RC calibration value, normally from the LPF calibration module. `RCCAL_LPF`,
    /// bits 6:4.
    #[bits(4..=6)]
    rc_calibration: u3,

    /// Power down the DC-offset cancellation DAC. `PD_DCODAC_LPF`, bit 2; reset 0.
    #[bits(2..=2)]
    dc_dac_off: bool,

    /// Power down the DC reference. `PD_DCOREF_LPF`, bit 1; reset 0.
    #[bits(1..=1)]
    dc_reference_off: bool,

    /// Power down the filter. `PD_FIL_LPF`, bit 0; reset 0.
    #[bits(0..=0)]
    filter_off: bool,
}
impl IndexedRegister for Control {
    type Map = Reg;
    type Index = Lpf;

    fn addr(lpf: Lpf) -> Reg {
        match lpf {
            Lpf::Tx => Reg::TxControl,
            Lpf::Rx => Reg::RxControl,
        }
    }
}

/// libusdr's `DACCAL` value, written on every bandwidth change.
const DC_DAC_CALIBRATION: u8 = 0x0c;

/// libusdr's `s_lpf_lut`: the first entry whose limit (kHz) covers the bandwidth wins.
/// libusdr marks it "TODO proper calibration".
#[rustfmt::skip]
const LUT: [(u32, LpfBandwidth, u8); 39] = [
    (1030, LpfBandwidth::Mhz0_75, 7),
    (1190, LpfBandwidth::Mhz0_75, 6),
    (1333, LpfBandwidth::Mhz0_75, 5),
    (1500, LpfBandwidth::Mhz0_75, 4),
    (1650, LpfBandwidth::Mhz0_75, 3),
    (1700, LpfBandwidth::Mhz0_875, 4),
    (1870, LpfBandwidth::Mhz0_875, 3),
    (2250, LpfBandwidth::Mhz1_25, 5),
    (2750, LpfBandwidth::Mhz1_25, 4),
    (3000, LpfBandwidth::Mhz1_375, 4),
    (3250, LpfBandwidth::Mhz1_5, 4),
    (3650, LpfBandwidth::Mhz1_5, 3),
    (4170, LpfBandwidth::Mhz1_92, 4),
    (4650, LpfBandwidth::Mhz1_92, 3),
    (5200, LpfBandwidth::Mhz1_92, 2),
    (5500, LpfBandwidth::Mhz2_5, 4),
    (5750, LpfBandwidth::Mhz3, 5),
    (6000, LpfBandwidth::Mhz2_75, 4),
    (6500, LpfBandwidth::Mhz3, 4),
    (7250, LpfBandwidth::Mhz3, 3),
    (7550, LpfBandwidth::Mhz3_5, 4),
    (8400, LpfBandwidth::Mhz3_5, 3),
    (9400, LpfBandwidth::Mhz4_375, 4),
    (10400, LpfBandwidth::Mhz4_375, 3),
    (11650, LpfBandwidth::Mhz5, 4),
    (12400, LpfBandwidth::Mhz5, 3),
    (14400, LpfBandwidth::Mhz6, 4),
    (15200, LpfBandwidth::Mhz6, 3),
    (16650, LpfBandwidth::Mhz7, 4),
    (18900, LpfBandwidth::Mhz7, 3),
    (21150, LpfBandwidth::Mhz10, 5),
    (24000, LpfBandwidth::Mhz10, 4),
    (25650, LpfBandwidth::Mhz10, 3),
    (28650, LpfBandwidth::Mhz14, 5),
    (31900, LpfBandwidth::Mhz14, 4),
    (35400, LpfBandwidth::Mhz14, 3),
    (39150, LpfBandwidth::Mhz14, 2),
    (44650, LpfBandwidth::Mhz14, 1),
    (47000, LpfBandwidth::Mhz14, 0),
];

/// The three register values libusdr writes for `bandwidth_hz` (`lms6002d_set_bandwidth`),
/// the same for either filter: the first [`LUT`] entry at or above it, or the filter
/// bypassed above 47 MHz.
pub(super) fn for_bandwidth(bandwidth_hz: u32) -> (Bandwidth, Bypass, Control) {
    let khz = bandwidth_hz / 1000;
    let entry = LUT.iter().find(|&&(limit_khz, ..)| khz <= limit_khz);
    let (bandwidth, rc, bypassed) = match entry {
        Some(&(_, bandwidth, rc)) => (bandwidth, rc, false),
        None => (LpfBandwidth::Mhz14, 0, true),
    };
    (
        Bandwidth::new()
            .with_bandwidth(bandwidth)
            .with_enabled(!bypassed),
        Bypass::new()
            .with_bypassed(bypassed)
            .with_dc_dac_calibration(u6::new(DC_DAC_CALIBRATION)),
        Control::new().with_rc_calibration(u3::new(rc)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The LUT's band numbers, in libusdr's order, against the codes the enum gives them.
    #[test]
    fn lut_bands_keep_libusdr_codes() {
        #[rustfmt::skip]
        let libusdr_bands: [u8; 39] = [
            15, 15, 15, 15, 15, 14, 14, 13, 13, 12, 11, 11, 10, 10, 10, 9, 7, 8, 7, 7, 6, 6, 5,
            5, 4, 4, 3, 3, 2, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0,
        ];
        let codes =
            LUT.map(|(_, bandwidth, _)| Bandwidth::new().with_bandwidth(bandwidth).to_raw() >> 2);
        assert_eq!(codes, libusdr_bands);
    }

    #[test]
    fn one_megahertz_uses_the_narrowest_filter() {
        // libusdr's words for 1 MHz: 0xD43E, 0xD50C, 0xD670.
        let (bandwidth, bypass, control) = for_bandwidth(1_000_000);
        assert_eq!(
            [bandwidth.to_raw(), bypass.to_raw(), control.to_raw()],
            [0x3e, 0x0c, 0x70]
        );
    }

    #[test]
    fn above_the_table_the_filter_is_bypassed() {
        let (bandwidth, bypass, control) = for_bandwidth(62_000_000);
        assert_eq!(
            [bandwidth.to_raw(), bypass.to_raw(), control.to_raw()],
            [0x00, 0x4c, 0x00]
        );
    }
}
