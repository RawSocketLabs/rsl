//! The gateware's control words the board's sequences write: the general-purpose output
//! banks (`IGPO_*`), the DSP chains' configuration ports (`REG_CFG_PHY_*`) with their NCOs,
//! resets, FIR microcode loads, corrections (RX DC correction, TX DC offset, IQ imbalance
//! terms) and the TX constant generator, as register accesses with no I/O.
//!
//! Source: `device/generic_usdr/generic_regs.h`, `dev_gpo_set`, `fpga_phy_regs`,
//! `_usdr_set_nco`, `usdr_set_samplerate_ex`, `usdr_set_rxdccorr`, `usdr_tx_dccorr`,
//! `usdr_rxupdate_cal`, `usdr_txupdate_cal` and `usdr_tx_gen_set` in
//! `device/m2_lm6_1/usdr_ctrl.c`; `fgearbox_load_ucode` in `ipblks/fgearbox.c`.

use core::time::Duration;

use bnb::{BitEnum, bitfield, u7, u12, u24};

use super::fir_tables;
use super::script::Script;
use super::sequence::{FpgaAccess, Sequence, SequenceAction, SequenceFailure};

/// The gateware's name in failures.
pub(crate) const FPGA: &str = "FPGA";

/// FPGA register latching one GPO bank per write.
const REG_GPO: u32 = 0;

/// A write to [`REG_GPO`]. Bit 31 set would load the I2C address LUT instead.
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GpoWrite {
    /// Which output bank to latch.
    #[bits(24..=30)]
    bank: u7,

    /// The bank's new value.
    #[bits(0..=7)]
    data: u8,
}

/// The board's general-purpose output banks (`IGPO_*` in `usdr_ctrl.h`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Gpo {
    /// LMS6002D reset, active low: 0 holds the chip in reset, 1 releases it. Held low from
    /// identification until the clocks and rails are up, and again at power-down.
    LmsReset = 0,

    /// Board mixer enable: 1 powers the mixer that lifts signals below 230 MHz into the
    /// LMS6002D's range, by adding Si5332 output 3's frequency, and feeds LNA3. libusdr
    /// drives it from bit 1 of an RX path's switch setting.
    RxMixerEnable = 1,

    /// TX antenna switch: picks the balun for PA1 (1) or PA2 (0).
    TxSwitch = 2,

    /// RX antenna switch: picks the balun for LNA1 (0) or LNA2 (1); libusdr also sets 1 on
    /// its LNA3 path.
    RxSwitch = 3,

    /// RF booster power, on just before the LMS6002D leaves reset and off at power-down.
    /// libusdr does not document what the line switches.
    Booster = 7,

    /// Status LED: lit while the board is powered up.
    Led = 8,

    /// DC correction enable (`IGPO_DCCORR`). A zero-IF receiver such as the LMS6002D leaves
    /// a spike at 0 Hz that has to be removed. libusdr sets this line once, at the end of
    /// power-up, and never clears it; its RX DC-offset correction is switched separately,
    /// through a PHY register (`usdr_set_rxdccorr`, and on every sample-rate change). What
    /// this line alone gates is not documented.
    DcCorrection = 9,

    /// Clock-measurement control (`IGPO_CLKMEAS`), written raw by libusdr's
    /// `/dm/sdr/0/clkmeas`; the vendored source does not document its bits.
    ClockMeasurement = 16,

    /// On-board 26 MHz oscillator enable, revision 3 only: the Si5332's reference there.
    EnableOscillator = 17,
}

impl Gpo {
    /// The write latching `value` into this bank.
    pub(crate) fn write(self, value: u8) -> FpgaAccess {
        let word = GpoWrite::new()
            .with_bank(u7::new(self as u8))
            .with_data(value);
        FpgaAccess::Write {
            reg: REG_GPO,
            value: word.to_raw(),
        }
    }
}

/// A DSP chain's configuration port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phy {
    /// The RX chain (`REG_CFG_PHY_0`).
    Rx = 56,

    /// The TX chain (`REG_CFG_PHY_1`).
    Tx = 57,
}

/// A sub-register behind a [`Phy`] port (libusdr's `fpga_phy_regs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SubReg {
    /// Chain reset and NCO reset controls; libusdr does not document the bits
    /// (`CFG_REG_RESET`).
    Reset = 0,

    /// DC correction (`CFG_REG_DCCTRL`): on the RX chain, the automatic correction's enable
    /// (`usdr_set_rxdccorr`); on the TX chain, the DC offset, Q in bits 23:12 and I in 11:0
    /// (`usdr_tx_dccorr`).
    DcControl = 1,

    /// IQ imbalance correction, I amplitude (`CFG_REG_IQIMB_0`).
    IqAmplitudeI = 2,

    /// IQ imbalance correction, Q amplitude (`CFG_REG_IQIMB_1`).
    IqAmplitudeQ = 3,

    /// IQ imbalance correction, I phase term (`CFG_REG_IQIMB_2`).
    IqPhaseI = 4,

    /// IQ imbalance correction, Q phase term (`CFG_REG_IQIMB_3`).
    IqPhaseQ = 5,

    /// The TX chain's constant generator, which replaces the samples with a fixed I/Q pair
    /// (`CFG_REG_DC_GEN`).
    Generator = 7,

    /// NCO 0 frequency word, low half (`CFG_REG_NCO0_L`).
    Nco0Low = 8,

    /// NCO 0 frequency word, high half (`CFG_REG_NCO0_H`).
    Nco0High = 9,

    /// NCO 1 frequency word, low half (`CFG_REG_NCO1_L`).
    Nco1Low = 10,

    /// NCO 1 frequency word, high half (`CFG_REG_NCO1_H`).
    Nco1High = 11,

    /// FIR microcode, one byte per write (`CFG_REG_DSP_LD`).
    FirLoad = 64,
}

/// One write to a [`Phy`] port (`MAKE_PHY_WR_REG`).
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PhyWrite {
    /// Which sub-register.
    #[bits(24..=30)]
    sub: u7,

    /// The value written.
    #[bits(0..=23)]
    data: u24,
}

/// The value of one write to [`SubReg::Generator`] (`MAKE_PHY_WR_SUB_REG`): which of the
/// generator's own registers, and its 16-bit value.
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GeneratorWrite {
    /// The generator register.
    #[bits(16..=23)]
    reg: GeneratorReg,

    /// The value written.
    #[bits(0..=15)]
    value: u16,
}

/// The constant generator's registers (libusdr's `CFG_REG_DC_GEN_*` in `usdr_ctrl.c`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
enum GeneratorReg {
    /// 1 replaces the TX samples with the constants, 0 passes them (`CFG_REG_DC_GEN_CTRL`).
    Control = 0,

    /// Channel A's I constant (`CFG_REG_DC_GEN_AI`).
    ChannelAI = 4,

    /// Channel A's Q constant (`CFG_REG_DC_GEN_AQ`).
    ChannelAQ = 5,

    /// Channel B's I constant (`CFG_REG_DC_GEN_BI`).
    ChannelBI = 6,

    /// Channel B's Q constant (`CFG_REG_DC_GEN_BQ`).
    ChannelBQ = 7,
}

/// One of a chain's two numerically controlled oscillators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Nco {
    /// NCO 0.
    Nco0,

    /// NCO 1.
    Nco1,
}

impl Phy {
    /// The port word writing `data` to a sub-register (`MAKE_PHY_WR_REG`).
    pub(crate) fn word(sub: SubReg, data: u32) -> u32 {
        PhyWrite::new()
            .with_sub(u7::new(sub as u8))
            .with_data(u24::new(data))
            .to_raw()
    }

    /// The write of one port word.
    pub(crate) const fn write_word(self, word: u32) -> FpgaAccess {
        FpgaAccess::Write {
            reg: self as u32,
            value: word,
        }
    }

    /// The write of `data` to a sub-register.
    pub(crate) fn write(self, sub: SubReg, data: u32) -> FpgaAccess {
        self.write_word(Self::word(sub, data))
    }

    /// The reset sub-register's write. libusdr writes fixed values (15 then 9 before a FIR
    /// load, 1 or 7 then 0 around a front-end reset) without documenting the bits.
    pub(crate) fn reset(self, bits: u32) -> FpgaAccess {
        self.write(SubReg::Reset, bits)
    }

    /// The write switching the chain's DC correction.
    pub(crate) fn dc_correction(self, on: bool) -> FpgaAccess {
        self.write(SubReg::DcControl, u32::from(on))
    }

    /// The writes setting an NCO's frequency word, then pulsing the NCO reset
    /// (`_usdr_set_nco`).
    pub(crate) fn nco(self, nco: Nco, word: i32) -> [FpgaAccess; 4] {
        let (low, high) = match nco {
            Nco::Nco0 => (SubReg::Nco0Low, SubReg::Nco0High),
            Nco::Nco1 => (SubReg::Nco1Low, SubReg::Nco1High),
        };
        let [b0, b1, b2, b3] = word.to_le_bytes();
        [
            self.write(low, u32::from(u16::from_le_bytes([b0, b1]))),
            self.write(high, u32::from(u16::from_le_bytes([b2, b3]))),
            self.reset(1 << 8),
            self.reset(0),
        ]
    }
}

impl Phy {
    /// Switches the chain's DC correction (`usdr_set_rxdccorr` on RX).
    pub(crate) fn set_dc_correction(self, on: bool) -> Script {
        Script::new().fpga(FPGA, self.dc_correction(on))
    }

    /// The port words writing the IQ imbalance correction `terms` (I and Q amplitudes,
    /// then I and Q phase terms), in that order. libusdr keeps each term's low 24 bits.
    pub(crate) fn iq_correction_words(terms: [i32; 4]) -> [u32; 4] {
        let subs = [
            SubReg::IqAmplitudeI,
            SubReg::IqAmplitudeQ,
            SubReg::IqPhaseI,
            SubReg::IqPhaseQ,
        ];
        let mut words = [0; 4];
        for ((word, sub), term) in words.iter_mut().zip(subs).zip(terms) {
            let [b0, b1, b2, _] = term.to_le_bytes();
            *word = Self::word(sub, u32::from_le_bytes([b0, b1, b2, 0]));
        }
        words
    }

    /// Writes the IQ imbalance correction `terms` (`usdr_rxupdate_cal`,
    /// `usdr_txupdate_cal`); the first failed write ends it.
    pub(crate) fn set_iq_correction(self, terms: [i32; 4]) -> Script {
        Self::iq_correction_words(terms)
            .into_iter()
            .fold(Script::new(), |script, word| {
                script.fpga(FPGA, self.write_word(word))
            })
    }

    /// Writes the TX chain's DC offset, 12-bit two's-complement `i` and `q`
    /// (`usdr_tx_dccorr`): Q in bits 23:12, I in 11:0.
    pub(crate) fn set_tx_dc_offset(i: u12, q: u12) -> Script {
        let data = u32::from(q.value()) << 12 | u32::from(i.value());
        Script::new().fpga(FPGA, Self::Tx.write(SubReg::DcControl, data))
    }

    /// The port word writing `value` to the constant generator's register `reg`.
    fn generator_word(reg: GeneratorReg, value: u16) -> u32 {
        let data = GeneratorWrite::new().with_reg(reg).with_value(value);
        Self::word(SubReg::Generator, data.to_raw())
    }

    /// The port word turning the constant generator off (`usdr_tx_gen_set(.., false, ..)`).
    pub(crate) fn generator_off_word() -> u32 {
        Self::generator_word(GeneratorReg::Control, 0)
    }

    /// Replaces the TX chain's samples with the constant `[i, q]` on both channels, or
    /// with `None` passes them again (`usdr_tx_gen_set` with channel mask 3): the enable,
    /// then, when on, channel A's I and Q and channel B's; the first failed write ends it.
    pub(crate) fn set_tx_constant(constant: Option<[i16; 2]>) -> Script {
        let write = |script: Script, reg: GeneratorReg, value: u16| {
            script.fpga(FPGA, Self::Tx.write_word(Self::generator_word(reg, value)))
        };
        let enable = u16::from(constant.is_some());
        let script = write(Script::new(), GeneratorReg::Control, enable);
        let Some([i, q]) = constant else {
            return script;
        };
        // Two's-complement bits, as libusdr's `int16_t` masked to 16 bits.
        let bits = |value: i16| u16::from_ne_bytes(value.to_ne_bytes());
        let (i, q) = (bits(i), bits(q));
        [
            (GeneratorReg::ChannelAI, i),
            (GeneratorReg::ChannelAQ, q),
            (GeneratorReg::ChannelBI, i),
            (GeneratorReg::ChannelBQ, q),
        ]
        .into_iter()
        .fold(script, |script, (reg, value)| write(script, reg, value))
    }
}

/// The DSP chains' rate-change factor, with the FIR microcode for it: the RX chain's
/// decimation and the TX chain's interpolation, which libusdr always sets alike.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decimation {
    /// No decimation: the FIR passes samples through.
    X1,

    /// Decimate by 2.
    X2,

    /// Decimate by 4.
    X4,

    /// Decimate by 8.
    X8,

    /// Decimate by 16.
    X16,

    /// Decimate by 32.
    X32,
}

impl Decimation {
    /// Every factor, smallest first.
    pub(crate) const ALL: [Self; 6] =
        [Self::X1, Self::X2, Self::X4, Self::X8, Self::X16, Self::X32];

    /// The factor as a number.
    pub(crate) const fn factor(self) -> u32 {
        match self {
            Self::X1 => 1,
            Self::X2 => 2,
            Self::X4 => 4,
            Self::X8 => 8,
            Self::X16 => 16,
            Self::X32 => 32,
        }
    }

    /// The RX FIR microcode libusdr loads for this factor on the uSDR's 7-series FPGA.
    const fn rx_fir(self) -> &'static [u8; 1280] {
        match self {
            Self::X1 => &fir_tables::BYPASS,
            Self::X2 => &fir_tables::DECIMATE_2,
            Self::X4 => &fir_tables::DECIMATE_4,
            Self::X8 => &fir_tables::DECIMATE_8,
            Self::X16 => &fir_tables::DECIMATE_16,
            Self::X32 => &fir_tables::DECIMATE_32,
        }
    }

    /// The TX FIR microcode libusdr loads for this interpolation factor on the uSDR's
    /// 7-series FPGA (`fgearbox_load_fir_i_ex` with `DSP_7SERIES`).
    const fn tx_fir(self) -> &'static [u8; 1280] {
        match self {
            Self::X1 => &fir_tables::BYPASS,
            Self::X2 => &fir_tables::INTERPOLATE_2,
            Self::X4 => &fir_tables::INTERPOLATE_4,
            Self::X8 => &fir_tables::INTERPOLATE_8,
            Self::X16 => &fir_tables::INTERPOLATE_16,
            Self::X32 => &fir_tables::INTERPOLATE_32,
        }
    }
}

/// A chain's FIR microcode load, a byte per write with a wait after each
/// (`fgearbox_load_fir_ex`): the RX decimator's 1 µs apart, the TX interpolator's 10 µs.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FirLoad {
    /// The chain's port.
    phy: Phy,
    /// The microcode.
    table: &'static [u8; 1280],
    /// The wait after each write.
    gap: Duration,
    /// Actions done: a write and a wait per byte.
    done: usize,
    /// A write failed.
    failed: bool,
}

impl FirLoad {
    /// The RX decimator's load for `decimation`.
    pub(crate) const fn rx(decimation: Decimation) -> Self {
        Self::new(Phy::Rx, decimation.rx_fir(), Duration::from_micros(1))
    }

    /// The TX interpolator's load for `interpolation`.
    pub(crate) const fn tx(interpolation: Decimation) -> Self {
        Self::new(Phy::Tx, interpolation.tx_fir(), Duration::from_micros(10))
    }

    /// The load of `table` into `phy`'s FIR, `gap` apart.
    const fn new(phy: Phy, table: &'static [u8; 1280], gap: Duration) -> Self {
        Self {
            phy,
            table,
            gap,
            done: 0,
            failed: false,
        }
    }
}

impl Sequence for FirLoad {
    fn next(&self) -> Option<SequenceAction> {
        if self.failed {
            return None;
        }
        let byte = self.table.get(self.done / 2)?;
        Some(if self.done % 2 == 0 {
            SequenceAction::Fpga(self.phy.write(SubReg::FirLoad, (*byte).into()))
        } else {
            SequenceAction::Sleep(self.gap)
        })
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        self.done += 1;
        super::script::checked(FPGA, action, result)
            .map(drop)
            .inspect_err(|_| self.failed = true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpo_writes_latch_the_bank_in_bits_30_to_24() {
        assert_eq!(
            Gpo::EnableOscillator.write(1),
            FpgaAccess::Write {
                reg: 0,
                value: 0x1100_0001
            }
        );
    }

    #[test]
    fn an_nco_word_is_split_into_halves_then_the_reset_pulses() {
        let writes = Phy::Tx.nco(Nco::Nco1, -2);
        let words = writes.map(|access| match access {
            FpgaAccess::Write { reg: 57, value } => value,
            other => panic!("only TX-port writes: {other:?}"),
        });
        // 0xfffffffe: the low half 0xfffe to sub-register 10, the high 0xffff to 11.
        assert_eq!(words, [0x0a00_fffe, 0x0b00_ffff, 0x0000_0100, 0]);
    }

    #[test]
    fn a_fir_load_writes_every_byte_with_its_wait_and_stops_at_a_failure() {
        let mut load = FirLoad::rx(Decimation::X2);
        let mut writes = 0;
        while let Some(action) = load.next() {
            if matches!(action, SequenceAction::Fpga(_)) {
                writes += 1;
            }
            load.complete::<()>(Ok(0)).unwrap();
        }
        assert_eq!(writes, 1280);
        let mut load = FirLoad::tx(Decimation::X2);
        assert!(load.complete(Err(())).is_err());
        assert_eq!(load.next(), None, "nothing follows a failed write");
    }
}
