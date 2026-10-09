//! The TX and RX synthesizers (datasheet `TxPLL`, registers 0x10-0x1F, and `RxPLL`,
//! 0x20-0x2F). The two blocks have identical maps 0x10 apart, so each register here is
//! indexed by [`Pll`].
//!
//! A synthesizer makes the local oscillator (LO) its mixer multiplies the signal by; the
//! LO frequency is the frequency the chain is tuned to. Each one locks a VCO to the
//! reference clock through a fractional-N divider, then divides the VCO down to the LO.
//! The chip has four VCOs per synthesizer, each covering part of the range, and output
//! dividers that set which octave the LO falls in ([`VcoSelect`]). Tuning picks a VCO and
//! divider from libusdr's table ([`vco_for`]), sets the fractional divider
//! ([`FractionalDivider`]), and searches the VCO's capacitor bank ([`VcoCapacitor`]) for
//! the codes where the tuning-voltage comparator ([`Comparator`]) reports lock. Power-up
//! only sets [`VcoRegulator`] and points the RX LO at LNA1's mixer.

use bnb::{BitEnum, bitfield, u3, u5, u6, u9, u23};

use super::rx_fe::Lna;
use super::spi::BlockReg;
use crate::chips::register::IndexedRegister;

/// Synthesizer register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(super) enum Reg {
    /// TX PLL `PLL_CFG`; see [`PllConfig`].
    TxPllConfig = 0x14,

    /// TX PLL `VCO_DIV` (no output-buffer field); see [`VcoSelect`].
    TxVcoSelect = 0x15,

    /// TX PLL `PFD_UP`; see [`ChargePump`].
    TxChargePump = 0x16,

    /// TX PLL `VCO_REG_PFD_U`; see [`VcoRegulator`].
    TxVcoRegulator = 0x17,

    /// TX PLL `VCO_REG_PFD_D`; see [`DownOffset`].
    TxDownOffset = 0x18,

    /// TX PLL `PLL_CFG2`; see [`VcoCapacitor`].
    TxVcoCapacitor = 0x19,

    /// TX PLL `VTUNE`; see [`Comparator`].
    TxComparator = 0x1a,

    /// RX PLL `NINT_NFRAC`, byte 0 (bits 31:24); see [`FractionalDivider`].
    RxDivider0 = 0x20,

    /// RX PLL `NINT_NFRAC`, byte 1 (bits 23:16).
    RxDivider1 = 0x21,

    /// RX PLL `NINT_NFRAC`, byte 2 (bits 15:8).
    RxDivider2 = 0x22,

    /// RX PLL `NINT_NFRAC`, byte 3 (bits 7:0).
    RxDivider3 = 0x23,

    /// RX PLL `PLL_CFG`.
    RxPllConfig = 0x24,

    /// RX PLL `VCO_DIV_BUFSEL`.
    RxVcoSelect = 0x25,

    /// RX PLL `PFD_UP`.
    RxChargePump = 0x26,

    /// RX PLL `VCO_REG_PFD_U`.
    RxVcoRegulator = 0x27,

    /// RX PLL `VCO_REG_PFD_D`.
    RxDownOffset = 0x28,

    /// RX PLL `PLL_CFG2`.
    RxVcoCapacitor = 0x29,

    /// RX PLL `VTUNE`.
    RxComparator = 0x2a,

    /// RX PLL `VCOCOMO`: the tuning-voltage comparator's power. Only `PD_SX` (bit 3, set
    /// powers the comparator down) is documented; libusdr writes whole bytes, so they stay
    /// raw: [`COMPARATOR_ON`] and [`COMPARATOR_OFF`].
    RxComparatorPower = 0x2b,
}

/// The RX fractional divider's bytes, most significant first.
pub(super) const RX_DIVIDER: [Reg; 4] = [
    Reg::RxDivider0,
    Reg::RxDivider1,
    Reg::RxDivider2,
    Reg::RxDivider3,
];

/// libusdr's `VCOCOMO` byte while it searches the capacitor bank: comparator powered.
pub(super) const COMPARATOR_ON: u8 = 0x76;

/// libusdr's `VCOCOMO` byte once tuned: as [`COMPARATOR_ON`] with `PD_SX` set.
pub(super) const COMPARATOR_OFF: u8 = 0x7e;

/// The lowest LO libusdr tunes, in Hz.
pub(super) const LOWEST_LO_HZ: u32 = 170_000_000;

/// libusdr's `s_vco_ranges`: the first row whose highest LO, in Hz, covers the LO picks
/// the VCO (`SELVCO`: 4 to 7 are VCO4 to VCO1) and the divider exponent (LO = VCO /
/// 2^(exponent + 1)). LOs above the last row also use it.
const VCO_RANGES: [(u32, u8, u8); 16] = [
    (285_625_000, 4, 3),
    (336_875_000, 5, 3),
    (405_000_000, 6, 3),
    (465_000_000, 7, 3),
    (571_250_000, 4, 2),
    (673_750_000, 5, 2),
    (810_000_000, 6, 2),
    (930_000_000, 7, 2),
    (1_142_500_000, 4, 1),
    (1_347_500_000, 5, 1),
    (1_620_000_000, 6, 1),
    (1_860_000_000, 7, 1),
    (2_285_000_000, 4, 0),
    (2_695_000_000, 5, 0),
    (3_240_000_000, 6, 0),
    (3_720_000_000, 7, 0),
];

/// The VCO (`SELVCO`) and divider exponent libusdr uses for `lo_hz`.
pub(super) fn vco_for(lo_hz: u32) -> (u3, u8) {
    let (_, vco, exponent) = VCO_RANGES[..VCO_RANGES.len() - 1]
        .iter()
        .find(|&&(highest, ..)| lo_hz <= highest)
        .unwrap_or(&VCO_RANGES[VCO_RANGES.len() - 1]);
    (u3::new(*vco), *exponent)
}

impl BlockReg for Reg {}

/// Which synthesizer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pll {
    /// The TX synthesizer.
    Tx,

    /// The RX synthesizer.
    Rx,
}

/// The VCO, its divider range, and (RX only) which LNA's LO buffer is driven. Datasheet
/// `VCO_DIV`, register 0x15 (TX), and `VCO_DIV_BUFSEL`, register 0x25 (RX).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct VcoSelect {
    /// Which of the four VCOs runs. `SELVCO`, bits 7:5.
    #[bits(5..=7)]
    vco: u3,

    /// Output divider range. `FRANGE`, bits 4:2.
    #[bits(2..=4)]
    divider_range: u3,

    /// Which LNA path's LO buffer is powered; RX PLL only. `SELOUT`, bits 1:0; reset LNA1.
    #[bits(0..=1)]
    lo_buffer: Lna,
}

impl IndexedRegister for VcoSelect {
    type Map = Reg;
    type Index = Pll;

    fn addr(pll: Pll) -> Reg {
        match pll {
            Pll::Tx => Reg::TxVcoSelect,
            Pll::Rx => Reg::RxVcoSelect,
        }
    }
}

/// The VCO's supply regulator and the charge pump's up-current offset. Datasheet
/// `VCO_REG_PFD_U`, registers 0x17 (TX) and 0x27 (RX).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct VcoRegulator {
    /// Bypass the VCO regulator. `BYPVCOREG`, bit 7.
    #[bits(7..=7)]
    regulator_bypassed: bool,

    /// Power the VCO regulator down. `PDVCOREG`, bit 6.
    #[bits(6..=6)]
    regulator_off: bool,

    /// Short the regulator band-gap resistor so it settles faster; disable once charged.
    /// `FSTVCOBG`, bit 5.
    #[bits(5..=5)]
    fast_bandgap_settling: bool,

    /// Charge-pump up-current offset, 10 µA per step. `OFFUP`, bits 4:0.
    #[bits(0..=4)]
    charge_pump_up_offset: u5,
}

impl VcoRegulator {
    /// Regulator bypassed and powered down, band gap settling fast, no charge-pump up
    /// offset: libusdr's power-up value for the TX PLL, 0xE0.
    pub(super) const BYPASSED: Self = Self::new()
        .with_regulator_bypassed(true)
        .with_regulator_off(true)
        .with_fast_bandgap_settling(true);

    /// [`Self::BYPASSED`] plus a 30 µA charge-pump up offset: libusdr's power-up value for
    /// the RX PLL, 0xE3. libusdr gives no reason for the offset, or for applying it to RX
    /// only.
    pub(super) const BYPASSED_WITH_UP_OFFSET: Self =
        Self::BYPASSED.with_charge_pump_up_offset(u5::new(3));
}

impl IndexedRegister for VcoRegulator {
    type Map = Reg;
    type Index = Pll;

    fn addr(pll: Pll) -> Reg {
        match pll {
            Pll::Tx => Reg::TxVcoRegulator,
            Pll::Rx => Reg::RxVcoRegulator,
        }
    }
}

impl VcoRegulator {
    /// Regulator bypassed and powered down, nothing else: libusdr's value while tuning,
    /// 0xC0, which replaces the power-up value.
    pub(super) const TUNING: Self = Self::new()
        .with_regulator_bypassed(true)
        .with_regulator_off(true);
}

/// The fractional-N feedback divider: VCO = reference × (`integer` + `fraction` / 2^23).
/// Datasheet `NINT_NFRAC`, four bytes at 0x10 (TX) and 0x20 (RX), most significant first.
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FractionalDivider {
    /// Integer part. `NINT`, bits 31:23.
    #[bits(23..=31)]
    integer: u9,

    /// Fractional part, in 2^-23. `NFRAC`, bits 22:0.
    #[bits(0..=22)]
    fraction: u23,
}

impl FractionalDivider {
    /// The divider that makes `vco_hz` from `reference_hz` (`lms6002d_pll_calc`): the
    /// fraction rounds down, and only the integer's low 9 bits fit.
    pub(super) fn for_vco(reference_hz: u32, vco_hz: u64) -> Self {
        let reference = u64::from(reference_hz);
        let integer = vco_hz / reference;
        let fraction = ((vco_hz - integer * reference) << 23) / reference;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "masked to 32 bits, as libusdr's uint32_t word"
        )]
        let word = ((integer << 23) & 0xff80_0000 | fraction & 0x7f_ffff) as u32;
        Self::from_raw(word)
    }
}

/// The synthesizer's loop setup. Datasheet `PLL_CFG`, registers 0x14 (TX) and 0x24 (RX).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PllConfig {
    /// Dither the delta-sigma modulator. `DITHEN`, bit 7.
    #[bits(7..=7)]
    dither_enabled: bool,

    /// How many bits to dither. `DITHN`, bits 6:4.
    #[bits(4..=6)]
    dither_bits: u3,

    /// Enable the PLL. `PLLEN`, bit 3.
    #[bits(3..=3)]
    enabled: bool,

    /// Bypass the delta-sigma modulator when the fraction is 0. `AUTOBYP`, bit 2.
    #[bits(2..=2)]
    auto_bypass: bool,

    /// Take power-down signals from the test-mode registers. `DECODE`, bit 1; reset 0.
    #[bits(1..=1)]
    test_mode_controls: bool,
}

impl PllConfig {
    /// libusdr's value while tuning, 0x9C: enabled, dithering on with `DITHN` 1 (the YAML
    /// does not give the code's bit count), auto-bypass on.
    pub(super) const TUNING: Self = Self::new()
        .with_dither_enabled(true)
        .with_dither_bits(u3::new(1))
        .with_enabled(true)
        .with_auto_bypass(true);
}

/// The phase detector's up pulses and the charge-pump current. Datasheet `PFD_UP`,
/// registers 0x16 (TX) and 0x26 (RX).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ChargePump {
    /// Enable the phase detector's up pulses. `EN_PFD_UP`, bit 7.
    #[bits(7..=7)]
    up_pulses: bool,

    /// Enable the test-signal output buffer. `OEN_TSTD_SX`, bit 6.
    #[bits(6..=6)]
    test_output: bool,

    /// Enable the test-signal pass. `PASSEN_TSTOD_SD`, bit 5.
    #[bits(5..=5)]
    test_pass: bool,

    /// Charge-pump current, 100 µA per step. `ICHP`, bits 4:0.
    #[bits(0..=4)]
    current: u5,
}

impl ChargePump {
    /// libusdr's value while tuning, 0x86: up pulses on, 600 µA.
    pub(super) const TUNING: Self = Self::new().with_up_pulses(true).with_current(u5::new(6));
}

/// The VCO regulator's upper voltage bits and the charge pump's down-current offset.
/// Datasheet `VCO_REG_PFD_D`, registers 0x18 (TX) and 0x28 (RX).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DownOffset {
    /// VCO regulator output voltage, high bits. `VOVCOREG_H`, bits 7:5.
    #[bits(5..=7)]
    regulator_voltage_high: u3,

    /// Charge-pump down-current offset, 10 µA per step. `OFFDOWN`, bits 4:0.
    #[bits(0..=4)]
    charge_pump_down_offset: u5,
}

impl DownOffset {
    /// libusdr's value while tuning, 0x02: a 20 µA down offset.
    pub(super) const TUNING: Self = Self::new().with_charge_pump_down_offset(u5::new(2));
}

/// The VCO's switched-capacitor bank, which sets where in its range it runs. Datasheet
/// `PLL_CFG2`, registers 0x19 (TX) and 0x29 (RX).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct VcoCapacitor {
    /// VCO regulator output voltage, low bit. `VOVCOREG_L`, bit 7.
    #[bits(7..=7)]
    regulator_voltage_low: bool,

    /// Capacitor code. `VCOCAP`, bits 5:0.
    #[bits(0..=5)]
    capacitor: u6,
}

/// The tuning-voltage comparator: whether the VCO's control voltage is inside its window.
/// Datasheet `VTUNE`, registers 0x1A (TX) and 0x2A (RX), read-only.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Comparator {
    /// The tuning voltage is above the window; libusdr raises the capacitor code.
    /// `VTUNE_H` (TX) or `COMPH` (RX), bit 7.
    #[bits(7..=7)]
    vtune_high: bool,

    /// The tuning voltage is below the window; libusdr lowers the capacitor code.
    /// `VTUNE_L` (TX) or `COMPL` (RX), bit 6.
    #[bits(6..=6)]
    vtune_low: bool,
}

/// Implements [`IndexedRegister`] over [`Pll`] for a register both synthesizers have.
macro_rules! per_pll {
    ($ty:ty, $tx:ident, $rx:ident) => {
        impl IndexedRegister for $ty {
            type Map = Reg;
            type Index = Pll;

            fn addr(pll: Pll) -> Reg {
                match pll {
                    Pll::Tx => Reg::$tx,
                    Pll::Rx => Reg::$rx,
                }
            }
        }
    };
}
per_pll!(PllConfig, TxPllConfig, RxPllConfig);
per_pll!(ChargePump, TxChargePump, RxChargePump);
per_pll!(DownOffset, TxDownOffset, RxDownOffset);
per_pll!(VcoCapacitor, TxVcoCapacitor, RxVcoCapacitor);
per_pll!(Comparator, TxComparator, RxComparator);

#[cfg(test)]
mod tests {
    use super::*;

    /// The board's reference.
    const REFERENCE_HZ: u32 = 26_000_000;

    // Expected words: libusdr's arithmetic worked by hand for a 26 MHz reference.

    #[test]
    fn one_gigahertz_uses_vco4_and_divides_by_4() {
        let (vco, exponent) = vco_for(1_000_000_000);
        assert_eq!((vco.value(), exponent), (4, 1));
        let divider = FractionalDivider::for_vco(REFERENCE_HZ, 1_000_000_000 << 2);
        assert_eq!(
            (divider.integer().value(), divider.fraction().value()),
            (153, 7_098_052)
        );
        assert_eq!(divider.to_raw().to_be_bytes(), [0x4c, 0xec, 0x4e, 0xc4]);
    }

    #[test]
    fn vco_table_boundaries_are_inclusive() {
        assert_eq!(vco_for(285_625_000).0.value(), 4);
        assert_eq!(vco_for(285_625_001).0.value(), 5);
    }

    #[test]
    fn above_the_table_the_last_row_applies() {
        assert_eq!(vco_for(4_000_000_000), (u3::new(7), 0));
    }

    #[test]
    fn tuning_values_are_libusdrs_bytes() {
        assert_eq!(
            [
                PllConfig::TUNING.to_raw(),
                ChargePump::TUNING.to_raw(),
                VcoRegulator::TUNING.to_raw(),
                DownOffset::TUNING.to_raw(),
            ],
            [0x9c, 0x86, 0xc0, 0x02]
        );
    }
}
