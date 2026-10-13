//! The `PCIe` front-end board (libusdr's `pciefe`, `device/ext_pciefe/ext_pciefe.c`): RX and
//! TX filter banks, a duplexer bank, an LNA, a PA, a 6 dB-step RX attenuator, a GPS module
//! and a reference oscillator trimmed by a DAC.
//!
//! Two TCA6416 expanders on the connector's I2C bus drive it: one switches the RF path,
//! the other the oscillator, GPS and LEDs. A `DACx0501` trims the oscillator. On the V1
//! board the attenuator and the TX-to-RX loopback also have fast lines on the connector's
//! GPIO pins, which libusdr drives instead of the expander once configured.

mod regs;

use std::time::Duration;

use rsl_lowlevel::{Bus, I2cAddr};

use self::regs::{
    AttenuatorCode, DuplexerCode, V0FrontEnd0, V0FrontEnd1, V0Gpio0, V0Gpio1, V0RxSelect1,
    V0RxSelect2, V0TxSelect1, V0TxSelect2, V1FrontEnd, V1Gpio0, V1Gpio1, V1RxSelect, V1TxSelect,
};
use crate::FRONT_END_BUS;
use crate::chips::dac80501::{Dac80501, Identity};
use crate::chips::tca6416::{Reg, Tca6416};
use crate::error::Error;
use crate::gpio::{FrontEndGpio, Pin, PinMode};

/// The board's name in errors and logs (libusdr's `pciefe`).
const NAME: &str = "pciefe";

/// The expander switching the RF path (`I2C_ADDR_FE`).
const FE_EXPANDER: Tca6416 = Tca6416::at(I2cAddr::new(FRONT_END_BUS, 0x20));
/// The expander switching the oscillator, GPS, LEDs and level translators
/// (`I2C_ADDR_GPIO`).
const GPIO_EXPANDER: Tca6416 = Tca6416::at(I2cAddr::new(FRONT_END_BUS, 0x21));
/// The oscillator-trim DAC (`I2C_ADDR_DAC`).
const DAC: Dac80501 = Dac80501::at(I2cAddr::new(FRONT_END_BUS, 0x48));

/// What libusdr takes for a read nobody acknowledged.
const NO_ACK: u32 = 0xbaad_beef;

/// The connector's I2C data line, left an input (`GPIO_SDA`). Also the attenuator's fast
/// line 0 on V1 (`GPIO_FATTN_0`).
const SDA: Pin = 0;
/// The connector's I2C clock line, left an input (`GPIO_SCL`). Also the attenuator's fast
/// line 1 on V1 (`GPIO_FATTN_1`).
const SCL: Pin = 1;
/// The 1PPS input (`GPIO_1PPS`).
const PPS: Pin = 2;
/// The GPS UART's transmit line (`GPIO_UART_TX`).
const UART_TX: Pin = 3;
/// The GPS UART's receive line (`GPIO_UART_RX`).
const UART_RX: Pin = 4;
/// The fast TX-to-RX loopback line, active low (`GPIO_FLBN`).
const LOOPBACK_N: Pin = 5;

/// Which board version is fitted; libusdr cannot tell them apart and assumes V1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum PciefeRevision {
    /// The first board, with Qorvo switches (libusdr's `V0_QORVO`, selected by a `v0`
    /// option prefix). No fast GPIO lines.
    V0,

    /// The current board (libusdr's `V1_HALF`), with fast attenuator and loopback lines.
    #[default]
    V1,
}

/// RX filter (libusdr's `rx_filters`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PciefeRxFilter {
    /// 1.2 GHz low-pass (`RX_LPF1200`).
    #[default]
    Lpf1200,

    /// 2.1 GHz low-pass (`RX_LPF2100`).
    Lpf2100,

    /// 2.1–3 GHz band-pass (`RX_BPF2100_3000`).
    Bpf2100To3000,

    /// 3–4.2 GHz band-pass (`RX_BPF3000_4200`).
    Bpf3000To4200,
}

/// TX filter (libusdr's `tx_filters`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PciefeTxFilter {
    /// 400 MHz low-pass (`TX_LPF400`).
    Lpf400,

    /// 1.2 GHz low-pass (`TX_LPF1200`).
    Lpf1200,

    /// 2.1 GHz low-pass (`TX_LPF2100`).
    Lpf2100,

    /// No band filter (`TX_BYPASS`); the V1 board still passes its 4 GHz low-pass.
    #[default]
    Bypass,
}

/// Duplexer path (libusdr's `trx_path`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PciefeDuplexer {
    /// No duplexer (`TRX_BYPASS`).
    #[default]
    Bypass,

    /// LTE band 2, PCS 1900 (`TRX_BAND2`).
    Band2,

    /// LTE band 3, DCS 1800 (`TRX_BAND3`).
    Band3,

    /// LTE band 5, GSM 850 (`TRX_BAND5`).
    Band5,

    /// LTE band 7, IMT-E (`TRX_BAND7`).
    Band7,

    /// LTE band 8, GSM 900 (`TRX_BAND8`).
    Band8,
}

/// RX attenuation, in 6 dB steps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PciefeAttenuation {
    /// None: insertion loss only.
    #[default]
    Db0,

    /// 6 dB.
    Db6,

    /// 12 dB.
    Db12,

    /// 18 dB.
    Db18,
}

/// A named RF path from libusdr's `known_path` table (its `path_` option): duplexer,
/// filters and amplifiers in one choice. libusdr's aliases are listed per variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PciefePath {
    /// Band 2 duplexer, LNA and PA (`band2`, `pcs`, `gsm1900`).
    Band2,
    /// Band 3 duplexer, LNA and PA (`band3`, `dcs`, `gsm1800`).
    Band3,
    /// Band 5 duplexer, LNA and PA (`band5`, `gsm850`).
    Band5,
    /// Band 7 duplexer, LNA and PA (`band7`, `imte`).
    Band7,
    /// Band 8 duplexer, LNA and PA (`band8`, `gsm900`).
    Band8,
    /// TX only through the 400 MHz low-pass, PA on (`txlpf400`).
    TxLpf400,
    /// TX only through the 1.2 GHz low-pass, PA on (`txlpf1200`).
    TxLpf1200,
    /// TX only through the 2.1 GHz low-pass, PA on (`txlpf2100`).
    TxLpf2100,
    /// TX only, filter bypassed, PA on (`txlpf4200`).
    TxLpf4200,
    /// RX only through the 1.2 GHz low-pass, LNA on (`rxlpf1200`).
    RxLpf1200,
    /// RX only through the 2.1 GHz low-pass, LNA on (`rxlpf2100`).
    RxLpf2100,
    /// RX only through the 2.1–3 GHz band-pass, LNA on (`rxbpf2100_3000`).
    RxBpf2100To3000,
    /// RX only through the 3–4.2 GHz band-pass, LNA on (`rxbpf3000_4200`).
    RxBpf3000To4200,
    /// Half duplex up to 400 MHz (`trx0_400`).
    Trx0To400,
    /// Half duplex 400 MHz to 1.2 GHz (`trx400_1200`).
    Trx400To1200,
    /// Half duplex 1.2 to 2.1 GHz (`trx1200_2100`).
    Trx1200To2100,
    /// Half duplex 2.1 to 3 GHz (`trx2100_3000`).
    Trx2100To3000,
    /// Half duplex 3 to 4.2 GHz (`trx3000_4200`).
    Trx3000To4200,
}

impl PciefePath {
    /// The path's duplexer, RX filter, TX filter, PA and LNA (`struct path_map`).
    const fn settings(self) -> (PciefeDuplexer, PciefeRxFilter, PciefeTxFilter, bool, bool) {
        use PciefeDuplexer as D;
        use PciefeRxFilter as R;
        use PciefeTxFilter as T;
        match self {
            Self::Band2 => (D::Band2, R::Lpf1200, T::Lpf400, true, true),
            Self::Band3 => (D::Band3, R::Lpf1200, T::Lpf400, true, true),
            Self::Band5 => (D::Band5, R::Lpf1200, T::Lpf400, true, true),
            Self::Band7 => (D::Band7, R::Lpf1200, T::Lpf400, true, true),
            Self::Band8 => (D::Band8, R::Lpf1200, T::Lpf400, true, true),
            Self::TxLpf400 => (D::Bypass, R::Lpf1200, T::Lpf400, true, false),
            Self::TxLpf1200 => (D::Bypass, R::Lpf1200, T::Lpf1200, true, false),
            Self::TxLpf2100 => (D::Bypass, R::Lpf1200, T::Lpf2100, true, false),
            Self::TxLpf4200 => (D::Bypass, R::Lpf1200, T::Bypass, true, false),
            Self::RxLpf1200 => (D::Bypass, R::Lpf1200, T::Lpf400, false, true),
            Self::RxLpf2100 => (D::Bypass, R::Lpf2100, T::Lpf400, false, true),
            Self::RxBpf2100To3000 => (D::Bypass, R::Bpf2100To3000, T::Lpf400, false, true),
            Self::RxBpf3000To4200 => (D::Bypass, R::Bpf3000To4200, T::Lpf400, false, true),
            Self::Trx0To400 => (D::Bypass, R::Lpf1200, T::Lpf400, true, true),
            Self::Trx400To1200 => (D::Bypass, R::Lpf1200, T::Lpf1200, true, true),
            Self::Trx1200To2100 => (D::Bypass, R::Lpf2100, T::Lpf2100, true, true),
            Self::Trx2100To3000 => (D::Bypass, R::Bpf2100To3000, T::Bypass, true, true),
            Self::Trx3000To4200 => (D::Bypass, R::Bpf3000To4200, T::Bypass, true, true),
        }
    }
}

/// Bring-up options for the `PCIe` front end: libusdr's `fe=pciefe...` options, typed.
/// Unset options keep libusdr's defaults.
///
/// libusdr rejects `fe=pciefe` with no options at all (its parser takes the empty
/// remainder for an unrecognised option); [`PciefeOptions::default`] behaves as
/// `fe=pciefe:v1` does. The `uart_` option, which only logs what the GPS sends, is not
/// ported.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct PciefeOptions {
    /// The board version (a `v0` prefix selects V0).
    pub revision: PciefeRevision,

    /// A named RF path (`path_`), applied before the other options.
    pub path: Option<PciefePath>,

    /// GPS module power (`gps_`); off by default.
    pub gps: Option<bool>,

    /// Reference oscillator power (`osc_`); off by default once the DAC is found.
    pub oscillator: Option<bool>,

    /// RX LNA (`lna_`).
    pub lna: Option<bool>,

    /// TX power amplifier (`pa_`).
    pub pa: Option<bool>,

    /// TX-to-RX loopback (`lb_`).
    pub loopback: Option<bool>,

    /// RX attenuation (`attn_`, which libusdr rounds to the nearest 6 dB step).
    pub attenuation: Option<PciefeAttenuation>,

    /// Oscillator DAC code (`dac_`).
    pub dac: Option<u16>,
}

/// One `PCIe` front-end control, as libusdr's `/debug/hw/pciefe_cmd/0/reg` commands set
/// them (`board_ext_pciefe_cmd_wr`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PciefeSetting {
    /// The oscillator DAC code (`FECMD_DAC`).
    Dac(u16),

    /// The TX filter (`FECMD_TXSEL`).
    TxFilter(PciefeTxFilter),

    /// The RX filter (`FECMD_RXSEL`).
    RxFilter(PciefeRxFilter),

    /// The duplexer path (`FECMD_DUPLSEL`).
    Duplexer(PciefeDuplexer),

    /// TX-to-RX loopback (`FECMD_LOOPBACK`).
    Loopback(bool),

    /// The four LED controls, LED 1 first (`FECMD_LED`), written as given; the board
    /// inverts them.
    Leds([bool; 4]),

    /// RX attenuation (`FECMD_ATTN`).
    Attenuation(PciefeAttenuation),

    /// RX LNA (`FECMD_CTRL_LNA`).
    Lna(bool),

    /// TX power amplifier (`FECMD_CTRL_PA`).
    Pa(bool),

    /// GPS module power (`FECMD_GPS`).
    Gps(bool),

    /// Reference oscillator power (`FECMD_OSC`).
    Oscillator(bool),
}

/// An attached `PCIe` front end and the settings libusdr tracks for it
/// (`board_ext_pciefe_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent switches, each set by its own libusdr command"
)]
pub struct Pciefe {
    /// Board version.
    revision: PciefeRevision,
    /// RX filter.
    rx: PciefeRxFilter,
    /// TX filter.
    tx: PciefeTxFilter,
    /// Duplexer path.
    duplexer: PciefeDuplexer,
    /// TX-to-RX loopback.
    loopback: bool,
    /// Reference oscillator on.
    oscillator: bool,
    /// GPS module on.
    gps: bool,
    /// RX LNA on.
    lna: bool,
    /// TX PA on.
    pa: bool,
    /// LED controls, LED 1 first.
    leds: [bool; 4],
    /// RX attenuation.
    attenuation: PciefeAttenuation,
}

impl Pciefe {
    /// Detects and brings up the board (`board_ext_pciefe_init`). `options` is `None` when
    /// probing without a hint: then a board that does not answer is simply absent
    /// (`Ok(None)`), while with a hint it is a failure, as in libusdr.
    pub(crate) fn init(
        bus: &mut dyn Bus,
        options: Option<&PciefeOptions>,
    ) -> Result<Option<Self>, Error> {
        let hinted = options.is_some();
        let options = options.copied().unwrap_or_default();
        FrontEndGpio::configure(bus, SDA, PinMode::Input)?;
        FrontEndGpio::configure(bus, SCL, PinMode::Input)?;
        FrontEndGpio::configure(bus, PPS, PinMode::Alternate0)?;

        // The oscillator stays on until the DAC has been identified: it powers the DAC.
        let mut board = Self {
            revision: options.revision,
            rx: PciefeRxFilter::Lpf1200,
            tx: PciefeTxFilter::Bypass,
            duplexer: PciefeDuplexer::Bypass,
            loopback: false,
            oscillator: true,
            gps: false,
            lna: false,
            pa: false,
            leds: [false; 4],
            attenuation: PciefeAttenuation::Db0,
        };
        let absent = |reason| {
            if hinted {
                Err(Error::Board {
                    board: NAME,
                    reason,
                })
            } else {
                Ok(None)
            }
        };

        for expander in [FE_EXPANDER, GPIO_EXPANDER] {
            if expander.read_word(bus, Reg::InputPort1)? == NO_ACK {
                return absent("an expander does not answer");
            }
        }
        GPIO_EXPANDER.write(bus, Reg::ConfigPort0, 0)?;
        GPIO_EXPANDER.write(bus, Reg::ConfigPort1, 0)?;
        GPIO_EXPANDER.write(bus, Reg::OutputPort0, 0)?;
        GPIO_EXPANDER.write(bus, Reg::OutputPort1, 0)?;
        FE_EXPANDER.write(bus, Reg::ConfigPort0, 0)?;
        FE_EXPANDER.write(bus, Reg::ConfigPort1, 0)?;
        FE_EXPANDER.write(bus, Reg::OutputPort0, 0)?;
        FE_EXPANDER.write(bus, Reg::OutputPort1, 1)?;
        board.update_power(bus)?;

        let mut dead = false;
        for expander in [FE_EXPANDER, GPIO_EXPANDER] {
            // libusdr keeps the word's low 16 bits.
            let inputs = expander.read_word(bus, Reg::InputPort0)? & 0xffff;
            dead |= inputs == 0;
        }
        if dead {
            return absent("an expander reads all inputs low");
        }

        // The DAC needs time to power up.
        bus.sleep(Duration::from_millis(10));
        match DAC.init(bus)? {
            Identity::Known => {}
            // `dac80501_init`'s `-ENODEV` reaches the V0 probe unchanged: no board at all,
            // hinted or not.
            Identity::Absent if board.revision == PciefeRevision::V0 => return Ok(None),
            Identity::Absent | Identity::Unknown => {
                return Err(Error::Board {
                    board: NAME,
                    reason: "the oscillator DAC is not recognised",
                });
            }
        }
        FrontEndGpio::configure(bus, PPS, PinMode::Alternate0)?;
        FrontEndGpio::configure(bus, UART_TX, PinMode::Alternate0)?;
        FrontEndGpio::configure(bus, UART_RX, PinMode::Alternate0)?;
        board.oscillator = false;

        if let Some(path) = options.path {
            let (duplexer, rx, tx, pa, lna) = path.settings();
            board.duplexer = duplexer;
            board.rx = rx;
            board.tx = tx;
            board.pa = pa;
            board.lna = lna;
        }
        board.gps = options.gps.unwrap_or(board.gps);
        board.oscillator = options.oscillator.unwrap_or(board.oscillator);
        board.lna = options.lna.unwrap_or(board.lna);
        board.pa = options.pa.unwrap_or(board.pa);
        board.loopback = options.loopback.unwrap_or(board.loopback);
        board.attenuation = options.attenuation.unwrap_or(board.attenuation);
        if let Some(code) = options.dac {
            DAC.set_code(bus, code)?;
        }

        if board.fast_lines() {
            FrontEndGpio::configure(bus, SDA, PinMode::Output)?;
            FrontEndGpio::configure(bus, SCL, PinMode::Output)?;
            board.drive_attenuator(bus)?;
            FrontEndGpio::configure(bus, LOOPBACK_N, PinMode::Output)?;
            board.drive_loopback(bus)?;
        }
        board.update_front_end(bus)?;
        board.update_power(bus)?;
        Ok(Some(board))
    }

    /// Sets the oscillator DAC's code (`board_ext_pciefe_set_dac`).
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn set_dac(bus: &mut dyn Bus, code: u16) -> Result<(), Error> {
        DAC.set_code(bus, code)
    }

    /// Applies one control (`board_ext_pciefe_cmd_wr`): most update both expanders; on V1
    /// the attenuator and loopback move their fast lines only.
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn apply(&mut self, bus: &mut dyn Bus, setting: PciefeSetting) -> Result<(), Error> {
        match setting {
            PciefeSetting::Dac(code) => return Self::set_dac(bus, code),
            PciefeSetting::TxFilter(tx) => self.tx = tx,
            PciefeSetting::RxFilter(rx) => self.rx = rx,
            PciefeSetting::Duplexer(duplexer) => self.duplexer = duplexer,
            PciefeSetting::Lna(on) => self.lna = on,
            PciefeSetting::Pa(on) => self.pa = on,
            PciefeSetting::Gps(on) => self.gps = on,
            PciefeSetting::Oscillator(on) => self.oscillator = on,
            PciefeSetting::Leds(leds) => self.leds = leds,
            PciefeSetting::Loopback(on) => {
                self.loopback = on;
                if self.fast_lines() {
                    return self.drive_loopback(bus);
                }
            }
            PciefeSetting::Attenuation(attenuation) => {
                self.attenuation = attenuation;
                if self.fast_lines() {
                    return self.drive_attenuator(bus);
                }
            }
        }
        self.update_front_end(bus)?;
        self.update_power(bus)
    }

    /// The V1 board's fast attenuator and loopback lines are in use (`cfg_fast_attn`,
    /// `cfg_fast_lb`).
    fn fast_lines(&self) -> bool {
        self.revision == PciefeRevision::V1
    }

    /// Drives the fast attenuator lines: step bit 0 on line 1, bit 1 on line 0.
    fn drive_attenuator(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        let step: u16 = match self.attenuation {
            PciefeAttenuation::Db0 => 0,
            PciefeAttenuation::Db6 => 1,
            PciefeAttenuation::Db12 => 2,
            PciefeAttenuation::Db18 => 3,
        };
        let levels = (step & 1) << SCL | (step >> 1 & 1) << SDA;
        FrontEndGpio::drive(bus, 1 << SCL | 1 << SDA, levels)
    }

    /// Drives the fast loopback line, low for loopback.
    fn drive_loopback(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        FrontEndGpio::drive(
            bus,
            1 << LOOPBACK_N,
            u16::from(!self.loopback) << LOOPBACK_N,
        )
    }

    /// The attenuator's register code.
    fn attenuator_code(&self) -> AttenuatorCode {
        match self.attenuation {
            PciefeAttenuation::Db0 => AttenuatorCode::Through,
            PciefeAttenuation::Db6 => AttenuatorCode::Db6,
            PciefeAttenuation::Db12 => AttenuatorCode::Db12,
            PciefeAttenuation::Db18 => AttenuatorCode::Db18,
        }
    }

    /// The duplexer's register code.
    fn duplexer_code(&self) -> DuplexerCode {
        match self.duplexer {
            PciefeDuplexer::Bypass => DuplexerCode::Bypass,
            PciefeDuplexer::Band2 => DuplexerCode::Band2,
            PciefeDuplexer::Band3 => DuplexerCode::Band3,
            PciefeDuplexer::Band5 => DuplexerCode::Band5,
            PciefeDuplexer::Band7 => DuplexerCode::Band7,
            PciefeDuplexer::Band8 => DuplexerCode::Band8,
        }
    }

    /// Writes the RF switches and amplifiers to the front-end expander
    /// (`board_ext_pciefe_updfe`).
    fn update_front_end(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        match self.revision {
            PciefeRevision::V1 => {
                let fe = V1FrontEnd::new()
                    .with_attenuator(self.attenuator_code())
                    .with_external_reference(!self.oscillator)
                    .with_pa1_off(!self.pa)
                    .with_lna2_off(!self.lna)
                    .with_lna1_off(!self.lna)
                    .with_duplexer(self.duplexer_code())
                    .with_pa2_off(!self.pa)
                    .with_rx(match self.rx {
                        PciefeRxFilter::Lpf1200 => V1RxSelect::Lpf1200,
                        PciefeRxFilter::Lpf2100 => V1RxSelect::Lpf2100,
                        PciefeRxFilter::Bpf2100To3000 => V1RxSelect::Bpf2100To3000,
                        PciefeRxFilter::Bpf3000To4200 => V1RxSelect::Bpf3000To4200,
                    })
                    .with_tx(match self.tx {
                        PciefeTxFilter::Lpf400 => V1TxSelect::Lpf400,
                        PciefeTxFilter::Lpf1200 => V1TxSelect::Lpf1200,
                        PciefeTxFilter::Lpf2100 => V1TxSelect::Lpf2100,
                        PciefeTxFilter::Bypass => V1TxSelect::Lpf4000,
                    });
                FE_EXPANDER.write_pair(bus, Reg::OutputPort0, fe.to_raw().to_le_bytes())
            }
            PciefeRevision::V0 => {
                let fe0 = V0FrontEnd0::new()
                    .with_tx2(match self.tx {
                        PciefeTxFilter::Lpf400 => V0TxSelect2::Lpf400,
                        PciefeTxFilter::Lpf1200 => V0TxSelect2::Lpf1200,
                        PciefeTxFilter::Lpf2100 => V0TxSelect2::Lpf2100,
                        PciefeTxFilter::Bypass => V0TxSelect2::Bypass,
                    })
                    .with_rx2(match self.rx {
                        PciefeRxFilter::Lpf1200 => V0RxSelect2::Lpf1200,
                        PciefeRxFilter::Lpf2100 => V0RxSelect2::Lpf2100,
                        PciefeRxFilter::Bpf2100To3000 => V0RxSelect2::Bpf2100To3000,
                        PciefeRxFilter::Bpf3000To4200 => V0RxSelect2::Bpf3000To4200,
                    })
                    .with_rx1(match self.rx {
                        PciefeRxFilter::Lpf1200 => V0RxSelect1::Lpf1200,
                        PciefeRxFilter::Lpf2100 => V0RxSelect1::Lpf2100,
                        PciefeRxFilter::Bpf2100To3000 => V0RxSelect1::Bpf2100To3000,
                        PciefeRxFilter::Bpf3000To4200 => V0RxSelect1::Bpf3000To4200,
                    })
                    .with_tx1(match self.tx {
                        PciefeTxFilter::Lpf400 => V0TxSelect1::Lpf400,
                        PciefeTxFilter::Lpf1200 => V0TxSelect1::Lpf1200,
                        PciefeTxFilter::Lpf2100 => V0TxSelect1::Lpf2100,
                        PciefeTxFilter::Bypass => V0TxSelect1::Bypass,
                    });
                // libusdr skips a register word of zero, which only `V0 FE0` (address 0)
                // could be; no filter choice makes its selectors all zero, so it is always
                // written.
                FE_EXPANDER.write(bus, Reg::OutputPort0, fe0.to_raw())?;
                // libusdr turns the PA on with the LNA as well.
                let fe1 = V0FrontEnd1::new()
                    .with_lna_on(self.lna)
                    .with_pa_on(self.pa || self.lna)
                    .with_attenuator(self.attenuator_code())
                    .with_loopback_off(!self.loopback)
                    .with_duplexer(self.duplexer_code());
                FE_EXPANDER.write(bus, Reg::OutputPort1, fe1.to_raw())
            }
        }
    }

    /// Writes the oscillator, GPS and LEDs to the GPIO expander (`board_ext_pciefe_updpwr`).
    fn update_power(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        let [led1, led2, led3, led4] = self.leds;
        let (port0, port1) = match self.revision {
            PciefeRevision::V1 => (
                V1Gpio0::new()
                    .with_cd_out(true)
                    .with_pins67_out(true)
                    .with_pins89_out(true)
                    .with_ab_out(true)
                    .with_led2(led2)
                    .with_led1(led1)
                    .with_gps_on(self.gps)
                    .with_oscillator_on(self.oscillator)
                    .to_raw(),
                V1Gpio1::new().with_led4(led4).with_led3(led3).to_raw(),
            ),
            PciefeRevision::V0 => (
                V0Gpio0::new()
                    .with_pins45_out(true)
                    .with_pins23_out(true)
                    .with_gps_on(self.gps)
                    .with_oscillator_on(self.oscillator)
                    .to_raw(),
                V0Gpio1::new()
                    .with_led4(led4)
                    .with_led3(led3)
                    .with_led2(led2)
                    .with_led1(led1)
                    .with_ab_out(true)
                    .with_pins89_out(true)
                    .with_pins01_out(true)
                    .with_pins67_out(true)
                    .to_raw(),
            ),
        };
        GPIO_EXPANDER.write(bus, Reg::OutputPort0, port0)?;
        GPIO_EXPANDER.write(bus, Reg::OutputPort1, port1)
    }
}
