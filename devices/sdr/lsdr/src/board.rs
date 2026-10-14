//! The `LSDr`'s wiring and setpoints, as the pinned libusdr's
//! `device/m2_lsdr/m2_da09_4_ad45_2.c` chooses them.

use hal::{I2cAddr, SpiAddr};

use crate::portable::supplies::SupplyPlan;
use crate::portable::tps6594::{Buck, BuckVoltage};
use crate::portable::wiring::Wiring;

/// `enum BUSIDX_m2_da09_4_ad45_2_rev000`: the TPS6594 at I2C bus 0, 0x48
/// (`I2C_BUS_TPS6594`), the Si549 at bus 0, 0x67 (`I2C_BUS_SI549`), the ADC sockets on SPI
/// buses 0 and 1 (`SPI_ADS42LBX9_0`/`_1`) and the LMK04832's on bus 2 (`SPI_LMK04832`).
pub(crate) const LSDR_WIRING: Wiring = Wiring {
    pmic: I2cAddr::new(0, 0x48),
    oscillator: I2cAddr::new(0, 0x67),
    adcs: [SpiAddr(0), SpiAddr(1)],
    clock_generator: SpiAddr(2),
};

/// The bucks open sets, in libusdr's order: 1 and 2 at 920 mV, 3 at 2200 mV, 4 at 1200 mV.
/// libusdr's source does not say what each rail feeds.
pub(crate) const LSDR_SUPPLIES: SupplyPlan = SupplyPlan {
    wiring: LSDR_WIRING,
    rails: [
        (Buck::B1, BuckVoltage::from_millivolts(920)),
        (Buck::B2, BuckVoltage::from_millivolts(920)),
        (Buck::B3, BuckVoltage::from_millivolts(2200)),
        (Buck::B4, BuckVoltage::from_millivolts(1200)),
    ],
};
