//! LMS6002D RF transceiver (source: `hw/lms6002d/lms6002d.c`, layouts from the generated
//! `def_lms6002d.h`).
//!
//! The SPI protocol is one 16-bit word per register: write flag, 7-bit address, data.
//! Several registers are write-only from the driver's point of view, so the driver keeps
//! the values it last wrote and updates fields in them.

use bnb::{BitEnum, bitfield, u2, u3, u4, u7};

use super::register::{Register, bitfield_register};
use crate::error::{BusContext, Error};
use crate::lowlevel::{Bus, SpiAddr};

/// One SPI transaction word.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SpiWord {
    /// Write (set) or read (clear).
    #[bits(15..=15)]
    write: bool,
    /// Register address.
    #[bits(8..=14)]
    addr: u7,
    /// Data written; zero for reads.
    #[bits(0..=7)]
    data: u8,
}

impl SpiWord {
    /// A write of a raw byte to `reg`.
    fn store_raw(reg: Reg, value: u8) -> Self {
        Self::new()
            .with_write(true)
            .with_addr(u7::new(reg.into()))
            .with_data(value)
    }

    /// A write of a typed register.
    fn store<R: Register<Map = Reg>>(value: R) -> Self {
        Self::store_raw(R::ADDR, value.to_byte())
    }

    /// A read of `reg`.
    fn load(reg: Reg) -> Self {
        Self::new().with_addr(u7::new(reg.into()))
    }
}

/// Register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
enum Reg {
    /// Chip version and revision.
    TopChipId = 0x04,
    /// Top-level enables; see [`TopEncfg`].
    TopEncfg = 0x05,
    /// Clock enables; see [`TopEnreg`].
    TopEnreg = 0x09,
    /// Crystal buffer and reference control; see [`TopPower`].
    TopPower = 0x0b,
    /// TX PLL VCO regulator and PFD up-offset.
    TxpllVcoRegPfdU = 0x17,
    /// RX VCO, divider range and output buffer; see [`RxpllVcoDivBufsel`].
    RxpllVcoDivBufsel = 0x25,
    /// RX PLL VCO regulator and PFD up-offset.
    RxpllVcoRegPfdU = 0x27,
    /// TX power amplifier selection; see [`TrfPaCtrl`].
    TrfPaCtrl = 0x44,
    /// TX LO buffer and driver bias.
    TrfCtrl4 = 0x47,
    /// RX ADC gain, common mode and buffer boost.
    AfeRxCtrl2 = 0x59,
    /// ADC/DAC interface polarity, IQ order and clock edges.
    AfeMiscCtrl = 0x5a,
    /// RX VGA2 enable and common mode.
    Rxvga2Ctrl = 0x64,
    /// RX front-end enable.
    RfeCtrl = 0x70,
    /// LNA gain and selection; see [`RfeGainLnaSel`].
    RfeGainLnaSel = 0x75,
    /// LNA load resistor.
    RfeRdlintLna = 0x79,
}

/// `TOP_ENCFG`: top-level enables.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TopEncfg {
    /// Decode control signals directly instead of from the top-level state.
    #[bits(7..=7)]
    decode: bool,
    /// Soft reset, active low.
    #[bits(5..=5)]
    sreset: bool,
    /// Top modules enable.
    #[bits(4..=4)]
    en: bool,
    /// Soft TX enable.
    #[bits(3..=3)]
    stxen: bool,
    /// Soft RX enable.
    #[bits(2..=2)]
    srxen: bool,
    /// TX/RX framing mode.
    #[bits(1..=1)]
    tfwmode: bool,
}
bitfield_register!(TopEncfg => Reg::TopEncfg);

/// `TOP_ENREG`: clock enables.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TopEnreg {
    /// RX output switch.
    #[bits(7..=7)]
    rxoutsw: bool,
    /// PLL clock output.
    #[bits(6..=6)]
    clk_pllclkout: bool,
    /// LPF calibration clock.
    #[bits(5..=5)]
    clk_lpf_cal: bool,
    /// RX VGA2 DC calibration clock.
    #[bits(4..=4)]
    clk_rx_vga2_dccal: bool,
    /// RX LPF DC calibration clock.
    #[bits(3..=3)]
    clk_rx_rpl_dccal: bool,
    /// RX PLL DSM SPI clock.
    #[bits(2..=2)]
    clk_rx_dsm_spi: bool,
    /// TX LPF DC calibration clock.
    #[bits(1..=1)]
    clk_tx_rpl_dccal: bool,
    /// TX PLL DSM SPI clock.
    #[bits(0..=0)]
    clk_tx_dsm_spi: bool,
}
bitfield_register!(TopEnreg => Reg::TopEnreg);

/// `TOP_POWER`: crystal buffer and reference control.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TopPower {
    /// Power down the crystal buffer.
    #[bits(4..=4)]
    pdxcobuf: bool,
    /// Crystal buffer self-biasing.
    #[bits(3..=3)]
    slfbxcobuf: bool,
    /// Bypass the crystal buffer.
    #[bits(2..=2)]
    bypxcobuf: bool,
    /// Power down the LPF calibration DC reference.
    #[bits(1..=1)]
    pd_dcoref_lpfcal: bool,
    /// Power up the RF loopback.
    #[bits(0..=0)]
    pu_rflb: bool,
}
bitfield_register!(TopPower => Reg::TopPower);

/// `RXPLL_VCO_DIV_BUFSEL`: RX VCO, frequency range and output buffer.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RxpllVcoDivBufsel {
    /// VCO select.
    #[bits(5..=7)]
    selvco: u3,
    /// Divider range.
    #[bits(2..=4)]
    frange: u3,
    /// Output buffer, which feeds the matching LNA.
    #[bits(0..=1)]
    selout: u2,
}
bitfield_register!(RxpllVcoDivBufsel => Reg::RxpllVcoDivBufsel);

/// `RFE_GAIN_LNA_SEL`: LNA gain and selection.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RfeGainLnaSel {
    /// LNA gain mode.
    #[bits(6..=7)]
    g_lna: u2,
    /// Active LNA.
    #[bits(4..=5)]
    lnasel: u2,
    /// LNA input capacitance.
    #[bits(0..=3)]
    cbe_lna: u4,
}
bitfield_register!(RfeGainLnaSel => Reg::RfeGainLnaSel);

/// `TRF_PA_CTRL`: TX power amplifier selection.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TrfPaCtrl {
    /// PA1/PA2 select.
    #[bits(3..=4)]
    en12: u2,
    /// Auxiliary PA enable.
    #[bits(2..=2)]
    enaux: bool,
}
bitfield_register!(TrfPaCtrl => Reg::TrfPaCtrl);

/// The RX input path: which LNA, and the matching PLL output buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RxPath {
    /// LNA1 (wideband on the uSDR).
    Lna1 = 1,
}

/// The TX output path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TxPath {
    /// PA1.
    Pa1 = 1,
}

/// An initialised LMS6002D and the register values the driver last wrote.
#[derive(Debug)]
pub(crate) struct Lms6002d {
    /// SPI target the chip is on.
    target: SpiAddr,
    /// Last `TOP_ENCFG` written.
    top_encfg: TopEncfg,
    /// Last `TOP_ENREG` written.
    top_enreg: TopEnreg,
    /// Last `RXPLL_VCO_DIV_BUFSEL` written.
    rxpll_bufsel: RxpllVcoDivBufsel,
    /// Last `RFE_GAIN_LNA_SEL` written.
    lna_sel: RfeGainLnaSel,
    /// Last `TRF_PA_CTRL` written.
    pa_ctrl: TrfPaCtrl,
}

impl Lms6002d {
    /// Reads the chip ID, then writes the power-up configuration (`lms6002d_create`).
    pub(crate) fn create(bus: &mut dyn Bus, target: SpiAddr) -> Result<Self, Error> {
        let lms = Self {
            target,
            top_encfg: TopEncfg::new()
                .with_sreset(true)
                .with_en(true)
                .with_tfwmode(true),
            top_enreg: TopEnreg::new(),
            rxpll_bufsel: RxpllVcoDivBufsel::new().with_selout(u2::new(1)),
            lna_sel: RfeGainLnaSel::from_raw(0xc0),
            pa_ctrl: TrfPaCtrl::new(),
        };
        let chip_id = lms.read(bus, Reg::TopChipId)?;
        lms.post(
            bus,
            &[
                SpiWord::store(TopEncfg::new().with_tfwmode(true)),
                SpiWord::store(lms.top_encfg),
                SpiWord::store(lms.top_enreg),
                // Crystal buffer self-biased, LPF calibration reference off.
                SpiWord::store(
                    TopPower::new()
                        .with_slfbxcobuf(true)
                        .with_pd_dcoref_lpfcal(true),
                ),
                SpiWord::store_raw(Reg::TxpllVcoRegPfdU, 0xe0),
                SpiWord::store_raw(Reg::RxpllVcoRegPfdU, 0xe3),
                SpiWord::store_raw(Reg::RfeCtrl, 0x01),
                // Lime FAQ v1.0r12, 5.27.
                SpiWord::store_raw(Reg::TrfCtrl4, 0x40),
                SpiWord::store_raw(Reg::AfeRxCtrl2, 0x29),
                SpiWord::store_raw(Reg::Rxvga2Ctrl, 0x36),
                SpiWord::store_raw(Reg::RfeRdlintLna, 0x37),
                // IQ order, negative polarity.
                SpiWord::store_raw(Reg::AfeMiscCtrl, 0xb0),
            ],
        )?;
        // libusdr checks the ID only after configuring.
        if chip_id == 0xff {
            return Err(Error::ChipMissing("LMS6002D"));
        }
        Ok(lms)
    }

    /// Enables or disables the TX chain's PLL clock and soft enable.
    pub(crate) fn set_tx_enabled(&mut self, bus: &mut dyn Bus, enable: bool) -> Result<(), Error> {
        self.top_enreg.set_clk_tx_dsm_spi(enable);
        self.top_encfg.set_stxen(enable);
        self.post_enables(bus)
    }

    /// Enables or disables the RX chain's PLL clock and soft enable.
    pub(crate) fn set_rx_enabled(&mut self, bus: &mut dyn Bus, enable: bool) -> Result<(), Error> {
        self.top_enreg.set_clk_rx_dsm_spi(enable);
        self.top_encfg.set_srxen(enable);
        self.post_enables(bus)
    }

    /// Selects the RX LNA and its PLL output buffer.
    pub(crate) fn set_rx_path(&mut self, bus: &mut dyn Bus, path: RxPath) -> Result<(), Error> {
        let path = u2::new(path as u8);
        self.rxpll_bufsel.set_selout(path);
        self.lna_sel.set_lnasel(path);
        self.post(
            bus,
            &[
                SpiWord::store(self.rxpll_bufsel),
                SpiWord::store(self.lna_sel),
            ],
        )
    }

    /// Selects the TX power amplifier.
    pub(crate) fn set_tx_path(&mut self, bus: &mut dyn Bus, path: TxPath) -> Result<(), Error> {
        self.pa_ctrl.set_en12(u2::new(path as u8));
        self.pa_ctrl.set_enaux(false);
        self.post(bus, &[SpiWord::store(self.pa_ctrl)])
    }

    /// Writes `TOP_ENREG` then `TOP_ENCFG` from the cached values.
    fn post_enables(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        self.post(
            bus,
            &[
                SpiWord::store(self.top_enreg),
                SpiWord::store(self.top_encfg),
            ],
        )
    }

    /// Reads one register.
    fn read(&self, bus: &mut dyn Bus, reg: Reg) -> Result<u8, Error> {
        let read = bus
            .spi32(self.target, SpiWord::load(reg).to_raw().into())
            .during("LMS6002D read")?;
        Ok(read.to_le_bytes()[0])
    }

    /// Sends register writes in order.
    fn post(&self, bus: &mut dyn Bus, words: &[SpiWord]) -> Result<(), Error> {
        for word in words {
            bus.spi32(self.target, word.to_raw().into())
                .during("LMS6002D write")?;
        }
        Ok(())
    }
}
