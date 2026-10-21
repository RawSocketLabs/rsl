//! The link's debug controls and the board's readings: `m2_lm7_1.c`'s
//! `/dm/sdr/0/phy_rx_lfsr`, `.../phy_tx_lfsr`, `.../phy_tx_iqsel`, `.../phy_rx_dly`,
//! `.../dumprx`, `/debug/clk_info`, `/dm/sensor/freqpps`, `/dm/sdr/0/usbclk`,
//! `/dm/sdr/0/vio` and `/dm/sdr/0/rfe/pwrdc` (`xsdr_ctrl.c`'s `xsdr_phy_*`,
//! `xsdr_config_rcvdly`, `xsdr_capture_rxiq`, `xsdr_clk_debug_info`, `xsdr_usbclk`,
//! `xsdr_set_vio`).

use std::time::Duration;

use hal::{FpgaAccess, IndexedRegister, RegisterAccess};
use lp8758::{Buck, BuckVoltage};

use super::quality::CAPTURE_SIZE;
use super::{Device, run};
use crate::board::PMIC;
use crate::error::Error;
use crate::portable::fpga::{Gpo, REG_CFG_PHY_0, RxPhyBank, TxPhy};
use crate::portable::hwid::Variant;
use crate::portable::step::Step;

/// The LFSR checkers' error counters' selector (`LFSR_CNTR_BER << 2`).
const BER: u8 = 2 << 2;
/// The RX clock's measurement (`IGPI_MEAS_RXCLK`, GPI word 4).
const REG_MEAS_RX_CLOCK: u32 = 16 + 16 / 4;
/// The TX clock's (`IGPI_MEAS_TXCLK`).
const REG_MEAS_TX_CLOCK: u32 = 16 + 20 / 4;
/// The 1PPS input's (`IGPI_CLK1PPS`).
const REG_MEAS_PPS: u32 = 16 + 24 / 4;
/// The DC estimator's two result words (`/dm/sdr/0/rfe/pwrdc`'s register 7, "unused").
const REG_RX_DC: u32 = 7;
/// The RX PHY's first delay bank (`PHY_REG_RXBANK_CLKDLY`).
const DELAY_BANK: u32 = 14;
/// The I/O supply's range on the `new_rev` boards (`xsdr_set_vio`).
const VIO_XSDR_MV: (u32, u32) = (1600, 2100);
/// On the XTRX.
const VIO_XTRX_MV: (u32, u32) = (1600, 3300);

/// One RX PHY delay line (`/dm/sdr/0/phy_rx_dly`'s type, `PHY_REG_RXBANK_CLKDLY` on).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RxPhyDelay {
    /// The RX clock's delay (`PHY_REG_RXBANK_CLKDLY`, type 0).
    Clock,

    /// The RX frame's (`PHY_REG_RXBANK_FRMDLY`, type 1).
    Frame,
}

/// The RX PHY's capture of both channels (`xsdr_capture_rxiq`): 128 samples each, I in
/// bits 11:0 and Q in 23:12, averaged over 256 captures.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RxCapture {
    /// Channel A's words.
    pub a: [u32; CAPTURE_SIZE],

    /// Channel B's.
    pub b: [u32; CAPTURE_SIZE],
}

/// The clock readings `/debug/clk_info` logs (`xsdr_clk_debug_info`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ClockReadings {
    /// The RX clock's measurement word (`IGPI_MEAS_RXCLK`; its count in bits 27:0).
    pub rx: u32,

    /// The TX clock's (`IGPI_MEAS_TXCLK`).
    pub tx: u32,

    /// The 1PPS input's (`IGPI_CLK1PPS`).
    pub pps: u32,

    /// The LFSR checkers' thirteen counters as the RX PHY reports them.
    pub lfsr: [u32; 13],
}

impl Device {
    /// Turns the RX PHY's LFSR checkers on or off (`/dm/sdr/0/phy_rx_lfsr`,
    /// `xsdr_phy_en_lfsr_checker_mimo`).
    ///
    /// # Errors
    ///
    /// [`Error::Bus`].
    pub fn set_rx_lfsr_checker(&mut self, on: bool) -> Result<(), Error> {
        self.rx_phy(true, RxPhyBank::IqAbCheck, 0, 0)?;
        self.rx_phy(true, RxPhyBank::LfsrCheck, 0, 0x0f)?;
        if on {
            self.rx_phy(true, RxPhyBank::LfsrCheck, 0, 0xf0)?;
        }
        Ok(())
    }

    /// The RX PHY's LFSR checkers' four error counters (`/dm/sdr/0/phy_rx_lfsr` read,
    /// `xsdr_phy_lfsr_mimo_state(d, LFSR_CNTR_BER, ..)`): each selected, 1 µs, then read.
    /// libusdr packs them, each saturated to 16 bits, into one number; they are given
    /// whole here.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`].
    pub fn rx_lfsr_errors(&mut self) -> Result<[u32; 4], Error> {
        let mut counters = [0; 4];
        for (counter, value) in (0..).zip(&mut counters) {
            self.rx_phy(false, RxPhyBank::LfsrCheck, BER | counter, 0)?;
            self.bus.sleep(Duration::from_micros(1));
            *value = self.read_reg(REG_CFG_PHY_0)?;
        }
        Ok(counters)
    }

    /// Runs or stops the TX PHY's generator, on its LFSR pattern or its A/B-I/Q table
    /// (`/dm/sdr/0/phy_tx_lfsr`: bit 0 run, bit 1 the LFSR; `xsdr_phy_en_lfsr_generator_mimo`).
    ///
    /// # Errors
    ///
    /// [`Error::Bus`].
    pub fn set_tx_generator_pattern(&mut self, run_it: bool, lfsr: bool) -> Result<(), Error> {
        self.tx_phy(TxPhy::LfsrGenerator, 0)?;
        self.bus.sleep(Duration::from_micros(1));
        self.tx_phy(TxPhy::IqAbGenerator, u32::from(!lfsr))?;
        self.tx_phy(TxPhy::LfsrGenerator, u32::from(run_it))
    }

    /// Sets which TX lanes carry I and Q (`/dm/sdr/0/phy_tx_iqsel`, `xsdr_phy_tx_iqsel`),
    /// libusdr's four-bit selection; the link calibration chooses it otherwise.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`].
    pub fn set_tx_iq_select(&mut self, lanes: u8) -> Result<(), Error> {
        self.tx_phy(TxPhy::IqSelect, lanes.into())
    }

    /// Sets an RX PHY delay line to `value`, 0 to 31 (`/dm/sdr/0/phy_rx_dly`,
    /// `xsdr_config_rcvdly`).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidDelay`] above 31 (libusdr masks it to five bits), before any bus
    /// access; [`Error::Bus`].
    pub fn set_rx_phy_delay(&mut self, line: RxPhyDelay, value: u8) -> Result<(), Error> {
        if value > 0x1f {
            return Err(Error::InvalidDelay(value));
        }
        let kind = match line {
            RxPhyDelay::Clock => 0,
            RxPhyDelay::Frame => 1,
        };
        let word = 1 << 31 | (DELAY_BANK + kind) << 24 | u32::from(value);
        self.write_reg(REG_CFG_PHY_0, word)
    }

    /// Captures both channels at the RX PHY (`/dm/sdr/0/dumprx`, `xsdr_capture_rxiq`),
    /// 256 captures averaged.
    ///
    /// # Errors
    ///
    /// [`Error::CaptureBusy`] for a capture word that stays busy; [`Error::Bus`].
    pub fn capture_rx(&mut self) -> Result<RxCapture, Error> {
        let [a, b] = self.capture_both()?;
        Ok(RxCapture { a, b })
    }

    /// The clocks' measurements and the LFSR counters (`/debug/clk_info`,
    /// `xsdr_clk_debug_info`): the three measurement words, then for each of thirteen
    /// counters four selections and a read, as libusdr makes them.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`].
    pub fn clock_readings(&mut self) -> Result<ClockReadings, Error> {
        let rx = self.read_reg(REG_MEAS_RX_CLOCK)?;
        let tx = self.read_reg(REG_MEAS_TX_CLOCK)?;
        let pps = self.read_reg(REG_MEAS_PPS)?;
        let mut lfsr = [0; 13];
        for (index, value) in (0_u8..).zip(&mut lfsr) {
            let address = (index & 0x3) << 2 | index >> 2;
            for _ in 0..4 {
                self.rx_phy(false, RxPhyBank::LfsrCheck, address, 0)?;
            }
            *value = self.read_reg(REG_CFG_PHY_0)?;
        }
        Ok(ClockReadings { rx, tx, pps, lfsr })
    }

    /// The 1PPS input's measurement: ticks between pulses (`/dm/sensor/freqpps`).
    ///
    /// # Errors
    ///
    /// [`Error::Bus`].
    pub fn pps_ticks(&mut self) -> Result<u32, Error> {
        self.read_reg(REG_MEAS_PPS)
    }

    /// Enables or disables the USB clock (`/dm/sdr/0/usbclk`, `xsdr_usbclk`: GPO
    /// `IGPO_USB_CLK_EN`), which libusdr offers for testing.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`].
    pub fn set_usb_clock(&mut self, on: bool) -> Result<(), Error> {
        run(
            &mut *self.bus,
            &mut Step::fpga(Gpo::UsbClock.write(u8::from(on))),
        )
    }

    /// Sets the I/O supply (`/dm/sdr/0/vio`, `xsdr_set_vio`), clamped as libusdr clamps
    /// it: 1.6 to 2.1 V on the XSDR, 1.6 to 3.3 V on the XTRX; the FPGA-side LP8758's
    /// channel 1. Returns the millivolts asked after the clamp.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`].
    pub fn set_io_voltage(&mut self, millivolts: u32) -> Result<u32, Error> {
        let (low, high) = if self.variant == Variant::Xtrx {
            VIO_XTRX_MV
        } else {
            VIO_XSDR_MV
        };
        let millivolts = millivolts.clamp(low, high);
        // Above 1.42 V libusdr's `s_get_vout` and SNVSAC6B give the same code.
        let write = RegisterAccess::write(
            PMIC,
            BuckVoltage::from_millivolts(millivolts).entry_at(Buck::B1),
        );
        run(&mut *self.bus, &mut Step::i2c(write, lp8758::NAME))?;
        Ok(millivolts)
    }

    /// The DC estimator's power reading in thousandths of a dB (`/dm/sdr/0/rfe/pwrdc`):
    /// its two words read as signed I and Q, `10000 * log10(I² + Q²)` truncated; `None`
    /// when both read 0, where libusdr reports 1000000.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`].
    pub fn rx_dc_power(&mut self) -> Result<Option<i64>, Error> {
        let mut words = [0; 2];
        self.bus
            .read_regs(REG_RX_DC, &mut words)
            .map_err(|source| Error::Bus {
                chip: crate::portable::fpga::NAME,
                access: hal::Access::Read,
                reg: REG_RX_DC,
                source,
            })?;
        let [i, q] = words.map(|word| f64::from(i32::from_ne_bytes(word.to_ne_bytes())));
        if i == 0.0 && q == 0.0 {
            return Ok(None);
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "libusdr's double stored to its uint64_t result, truncated"
        )]
        let millidb = (1000.0 * 10.0 * (i * i + q * q).log10()) as i64;
        Ok(Some(millidb))
    }

    /// `xsdr_phy_rx_reg`.
    fn rx_phy(
        &mut self,
        write: bool,
        bank: RxPhyBank,
        address: u8,
        value: u16,
    ) -> Result<(), Error> {
        run(
            &mut *self.bus,
            &mut Step::fpga(bank.access(write, address, value)),
        )
    }

    /// `xsdr_phy_tx_reg`.
    fn tx_phy(&mut self, register: TxPhy, value: u32) -> Result<(), Error> {
        run(&mut *self.bus, &mut Step::fpga(register.write(value)))
    }

    /// One gateware register's read.
    fn read_reg(&mut self, reg: u32) -> Result<u32, Error> {
        let mut read = Step::fpga(FpgaAccess::Read { reg });
        run(&mut *self.bus, &mut read)?;
        Ok(read.value().unwrap_or_default())
    }

    /// One gateware register's write.
    fn write_reg(&mut self, reg: u32, value: u32) -> Result<(), Error> {
        run(
            &mut *self.bus,
            &mut Step::fpga(FpgaAccess::Write { reg, value }),
        )
    }
}
