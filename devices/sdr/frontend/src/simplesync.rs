//! The `SimpleSync` clock board (libusdr's `simplesync`,
//! `device/ext_simplesync/ext_simplesync.c`): an LMK05318 fanning a disciplined reference
//! out to four SDRs.
//!
//! Outputs 4 to 7 carry a 25 MHz reference each (`CLK B`, `C`, `D`, `A`), from APLL1;
//! outputs 0 to 3 (`RF D`, `A`, `B`, `C`) carry an optional measurement LO from APLL2, off
//! until one is requested. libusdr finds the board by the LMK05318's ID, read once the
//! clock tree and GPS are powered.

use std::time::Duration;

use deps::tracing::info;
use hal::{Bus, I2cAddr};
use lmk05318::plan::{Affinity, OutputFormat, OutputRequest, Xo, XoInput};

use crate::FRONT_END_BUS;
use crate::chips::clock_id;
use crate::chips::lmk05318::Lmk05318;
use crate::error::Error;
use crate::gpio::{FrontEndGpio, GpioCommand, Pin, PinMode};

/// The clock synchronizer (`I2C_ADDR_LMK`).
const LMK05318: I2cAddr = I2cAddr::new(FRONT_END_BUS, 0x65);

/// The clock tree's power enable (`GPIO_ENCLKPWR`).
const EN_CLOCK_POWER: Pin = 5;
/// The GPS module's enable (`GPIO_ENGPS`).
const EN_GPS: Pin = 6;
/// Both enables.
const ENABLES: u16 = 1 << EN_CLOCK_POWER | 1 << EN_GPS;

/// The connector setup: its I2C lines to the core, the SYSREF generator out, both enables
/// as outputs, then both driven high.
const SETUP: [GpioCommand; 6] = [
    GpioCommand::Configure(0, PinMode::Alternate0),
    GpioCommand::Configure(1, PinMode::Alternate0),
    GpioCommand::Configure(7, PinMode::Alternate0),
    GpioCommand::Configure(EN_CLOCK_POWER, PinMode::Output),
    GpioCommand::Configure(EN_GPS, PinMode::Output),
    GpioCommand::Drive {
        mask: ENABLES,
        levels: ENABLES,
    },
];

/// How long libusdr lets the board power up before reading the synchronizer's ID.
const POWER_UP: Duration = Duration::from_millis(50);

/// The LMK05318's XO: 26 MHz LVCMOS.
const XO: Xo = Xo {
    hz: 26_000_000,
    input: XoInput::Cmos,
    detector_bypass: false,
};

/// The reference on outputs 4 to 7, in Hz.
const REFERENCE_HZ: u32 = 25_000_000;

/// The lowest measurement LO; below it outputs 0 to 3 are turned off (`LO_FREQ_CUTOFF`:
/// APLL2's lowest VCO over its largest post-divider and output divider, 3.07 MHz, rounded
/// up), in Hz.
const LO_CUTOFF_HZ: u32 = 3_500_000;

/// How long each lock wait lasts (10 ms).
const LOCK_TIMEOUT: Duration = Duration::from_millis(10);

/// The measurement LO's outputs.
const LO_OUTPUTS: [u8; 4] = [0, 1, 2, 3];

/// A `SimpleSync` board that came up, with its clock synchronizer's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimpleSync {
    /// The clock synchronizer.
    lmk: Lmk05318,
}

impl SimpleSync {
    /// Looks for the board and brings it up (`board_ext_simplesync_init`): configures the
    /// connector's pins, powers the clock tree and GPS, waits and reads the synchronizer's
    /// ID; then the LMK05318 with the references on outputs 4 to 7, the LO outputs off,
    /// both APLLs locked and the outputs synchronized. `None` when the ID is not the
    /// LMK05318's; the board stays powered then, as libusdr leaves it.
    ///
    /// # Errors
    ///
    /// [`Error::ClockUnlocked`] if an APLL does not lock; any bus failure.
    pub(crate) fn init(bus: &mut dyn Bus) -> Result<Option<Self>, Error> {
        FrontEndGpio::run(bus, &SETUP)?;
        bus.sleep(POWER_UP);
        if clock_id::read(bus, LMK05318, lmk05318::NAME)? != lmk05318::ID {
            return Ok(None);
        }

        let references = [4, 5, 6, 7].map(|port| {
            OutputRequest::new(port, REFERENCE_HZ, OutputFormat::LvcmosPN)
                .with_affinity(Affinity::Apll1)
        });
        let mut lmk = Lmk05318::at(LMK05318);
        lmk.configure(bus, XO, &references)?;
        let board = Self { lmk };
        let ready = board
            .power_down_lo(bus)
            .and_then(|()| board.lmk.wait_apll1_lock(bus, LOCK_TIMEOUT))
            .and_then(|()| board.lmk.wait_apll2_lock(bus, LOCK_TIMEOUT))
            .and_then(|()| board.lmk.sync(bus));
        board.lmk.log_lock(bus);
        ready?;
        info!("SimpleSync LMK05318 initialized");
        Ok(Some(board))
    }

    /// Puts a measurement LO of `hz` on outputs 0 to 3 from APLL2, or turns them off below
    /// 3.5 MHz (`simplesync_tune_lo`), waiting for APLL2 and synchronizing the outputs.
    ///
    /// # Errors
    ///
    /// [`Error::ClockPlan`] when the LMK05318 cannot reach `hz` (nothing is written);
    /// [`Error::ClockUnlocked`] if APLL2 does not lock; any bus failure.
    pub fn tune_lo(&mut self, bus: &mut dyn Bus, hz: u32) -> Result<(), Error> {
        if hz < LO_CUTOFF_HZ {
            return self.power_down_lo(bus);
        }
        let lo = LO_OUTPUTS.map(|port| {
            OutputRequest::new(port, hz, OutputFormat::Lvds).with_affinity(Affinity::Apll2)
        });
        self.lmk.plan(&lo)?;
        self.lmk.write_staged(bus)?;
        let synced = self
            .lmk
            .wait_apll2_lock(bus, LOCK_TIMEOUT)
            .and_then(|()| self.lmk.sync(bus));
        self.lmk.log_lock(bus);
        synced
    }

    /// Turns outputs 0 to 3 off (`simplesync_pd_low_chs`).
    fn power_down_lo(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        LO_OUTPUTS
            .into_iter()
            .try_for_each(|port| self.lmk.disable_output(bus, port))
    }
}
