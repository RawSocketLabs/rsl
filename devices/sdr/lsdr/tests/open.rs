//! Opening, powering and closing an `LSDr` on the simulated board: the board state libusdr's
//! sequences leave (`device/m2_lsdr/m2_da09_4_ad45_2.c`). Operation parity with libusdr
//! itself is `oracle/tests/session_parity.rs`.

mod common;

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use common::sim_bus::SimBus;
use rsl_lsdr::lowlevel::{Bus, BusError, I2cAddr, SpiAddr};
use rsl_lsdr::{Access, Device, Error, Fitted, OpenOptions, PgaGain};
use rsl_lsdr_sim::{I2cAddress, Op, SimBoard, Variant};

/// GPO bank `IGPO_BANK_CFG_ADC`: bits 1:0 hold the ADCs in reset.
const GPO_CFG_ADC: u8 = 2;
/// GPO bank `IGPO_BANK_PGA_LE`.
const GPO_PGA_LE: u8 = 3;
/// GPO bank `IGPO_BANK_PGA_GAIN`.
const GPO_PGA_GAIN: u8 = 4;
/// The TPS6594's `BUCK1_VOUT_1` .. `BUCK4_VOUT_1`.
const BUCK_VOUT: [u8; 4] = [0x0e, 0x10, 0x12, 0x14];
/// `ADS42LBx9` `REG_PWR_CONTROL`.
const ADC_PWR_CONTROL: u8 = 0x08;

/// The bus accesses (register, SPI and I2C calls, counted from 0) a [`Faulty`] bus fails.
#[derive(Debug, Default)]
struct Faults {
    /// Accesses made so far.
    calls: usize,

    /// The accesses to fail, by count.
    failing: Vec<usize>,
}

/// A [`SimBus`] whose chosen accesses fail with a timeout before reaching the board, so the
/// board's trace holds only the accesses that succeeded.
#[derive(Clone)]
struct Faulty {
    /// The board.
    sim: SimBus,

    /// Which accesses fail; shared, so a test can choose them after open.
    faults: Arc<Mutex<Faults>>,
}

impl Faulty {
    /// A fresh `variant` board on which nothing fails yet.
    fn new(variant: Variant) -> Self {
        Self {
            sim: SimBus::new(SimBoard::new(variant)),
            faults: Arc::default(),
        }
    }

    /// Fails the accesses `offsets` after the ones made so far.
    fn fail_next(&self, offsets: &[usize]) {
        let mut faults = self.faults.lock().unwrap_or_else(PoisonError::into_inner);
        let calls = faults.calls;
        faults.failing = offsets.iter().map(|offset| calls + offset).collect();
    }

    /// Counts one access, and whether it fails.
    fn access(&self) -> Result<(), BusError> {
        let mut faults = self.faults.lock().unwrap_or_else(PoisonError::into_inner);
        let call = faults.calls;
        faults.calls += 1;
        if faults.failing.contains(&call) {
            Err(BusError::Timeout)
        } else {
            Ok(())
        }
    }
}

impl Bus for Faulty {
    fn read_regs(&mut self, addr: u32, out: &mut [u32]) -> Result<(), BusError> {
        self.access()?;
        self.sim.read_regs(addr, out)
    }

    fn write_regs(&mut self, addr: u32, values: &[u32]) -> Result<(), BusError> {
        self.access()?;
        self.sim.write_regs(addr, values)
    }

    fn spi32(&mut self, target: SpiAddr, word: u32) -> Result<u32, BusError> {
        self.access()?;
        self.sim.spi32(target, word)
    }

    fn i2c(&mut self, dev: I2cAddr, write: &[u8], read: &mut [u8]) -> Result<(), BusError> {
        self.access()?;
        self.sim.i2c(dev, write, read)
    }

    fn sleep(&mut self, duration: Duration) {
        self.sim.sleep(duration);
    }
}

/// A board of `variant`, opened with `options`, and the bus to inspect it through.
fn open(variant: Variant, options: OpenOptions) -> (Device, SimBus) {
    let sim = SimBus::new(SimBoard::new(variant));
    let device = Device::open(sim.clone(), options).expect("an LSDr opens");
    (device, sim)
}

#[test]
fn open_sets_the_rails_finds_the_adc_and_leaves_it_unsupplied_and_unclocked() {
    let (device, sim) = open(Variant::SingleAdc, OpenOptions::new());
    assert_eq!(
        device.fitted(),
        Fitted {
            adcs: [true, false],
            clock_generator: false,
        }
    );
    let board = sim.board();
    // libusdr's `tps6594_buck_vout`: 920 mV -> 0x0F + 320 / 5, 2200 mV -> 0xAB + 540 / 20,
    // 1200 mV -> 0x73 + 100 / 10.
    let rails = BUCK_VOUT.map(|reg| board.pmic_reg(reg));
    assert_eq!(rails, [0x4f, 0x4f, 0xc6, 0x7d]);
    assert!(!board.adc_supply_enabled(), "LDO4 is off after the probes");
    assert!(!board.adc_clock_enabled(), "the Si549 output stays gated");
    assert_eq!(
        board.gpo(GPO_CFG_ADC),
        None,
        "no ADC reset line moves at open"
    );
}

#[test]
fn open_ends_with_the_front_end_probe_on_an_empty_connector() {
    let (_, sim) = open(Variant::SingleAdc, OpenOptions::new());
    let board = sim.board();
    let trace = board.trace();
    let probe_start = trace
        .iter()
        .position(|op| matches!(op, Op::RegWrite { addr: 3, .. }))
        .expect("open writes the connector's GPIO core");
    let probe = &trace[probe_start..];
    assert!(
        probe.iter().all(|op| match op {
            Op::RegWrite { addr, .. } => [3, 58].contains(addr),
            Op::Spi { bus, .. } => *bus == 4,
            Op::I2c { addr, .. } => addr.core == 1,
            Op::Sleep { .. } | Op::RegRead { .. } => false,
        }),
        "the probe stays on the connector: GPIO core, SPI extension, SPI 4, I2C core 1"
    );
    let devices: Vec<String> = probe
        .iter()
        .filter_map(|op| match op {
            Op::I2c { addr, .. } => Some(addr.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(
        devices,
        ["1:0:0x4e", "1:0:0x22", "1:0:0x23", "1:1:0x22", "1:1:0x23"],
        "fe4ch4007200's sensor, then its four expanders"
    );
}

#[test]
fn the_clock_generator_is_tested_only_when_asked_for() {
    let (device, sim) = open(Variant::DualAdcClockGen, OpenOptions::new());
    assert_eq!(device.fitted().adcs, [true, true]);
    assert!(!device.fitted().clock_generator);
    assert!(
        !sim.board()
            .trace()
            .iter()
            .any(|op| matches!(op, Op::Spi { bus: 2, .. })),
        "without lmk=1 the LMK04832 is not touched"
    );

    let (device, _) = open(
        Variant::DualAdcClockGen,
        OpenOptions::new().probe_clock_generator(true),
    );
    assert!(device.fitted().clock_generator, "the sim's LMK04832 passes");

    let (device, _) = open(
        Variant::SingleAdc,
        OpenOptions::new().probe_clock_generator(true),
    );
    assert!(
        !device.fitted().clock_generator,
        "an empty socket opens, without a clock generator"
    );
}

#[test]
fn a_dead_link_is_refused_having_read_only_the_status_word() {
    let sim = SimBus::new(SimBoard::new(Variant::SingleAdc).dead());
    assert!(matches!(
        Device::open(sim.clone(), OpenOptions::new()),
        Err(Error::DeadLink)
    ));
    assert_eq!(sim.board().trace().len(), 1);
}

#[test]
fn power_supplies_and_releases_the_adcs_and_power_down_holds_them_unsupplied() {
    let (mut device, sim) = open(Variant::SingleAdc, OpenOptions::new());
    device.set_power(true).expect("the ADCs power up");
    {
        let board = sim.board();
        assert!(board.adc_supply_enabled());
        assert_eq!(board.gpo(GPO_CFG_ADC), Some(0), "both ADCs out of reset");
        assert!(!board.adc_clock_enabled(), "power does not start the clock");
    }
    device.set_power(false).expect("the ADCs power down");
    let board = sim.board();
    assert!(!board.adc_supply_enabled());
    assert_eq!(
        board.gpo(GPO_CFG_ADC),
        Some(0b11),
        "both ADCs held in reset"
    );
}

#[test]
fn a_pga_gain_is_written_as_libusdrs_code_and_latched() {
    let (mut device, sim) = open(Variant::SingleAdc, OpenOptions::new());
    let gain = PgaGain::new(10).expect("within 0..=31");
    device.set_rx_pga_gain(gain).expect("the gain is set");
    let board = sim.board();
    assert_eq!(board.gpo(GPO_PGA_GAIN), Some(63 - 2 * 10));
    assert_eq!(
        board.gpo(GPO_PGA_LE),
        Some(0),
        "the latch is released again"
    );
}

#[test]
fn close_gates_the_clock_cuts_the_supply_and_powers_both_sockets_down() {
    let (mut device, sim) = open(Variant::DualAdcClockGen, OpenOptions::new());
    device.set_power(true).expect("the ADCs power up");
    device.close().expect("the board closes");
    let board = sim.board();
    assert!(!board.adc_clock_enabled());
    assert!(!board.adc_supply_enabled());
    for adc in 0..2 {
        assert_eq!(
            board.adc_reg(adc, ADC_PWR_CONTROL),
            Some(0xe0),
            "ADC {adc}: both channels and the chip powered down"
        );
    }
}

#[test]
fn a_failed_liveness_read_fails_open_before_anything_else() {
    let bus = Faulty::new(Variant::SingleAdc);
    bus.fail_next(&[0]);
    let refusal = Device::open(bus.clone(), OpenOptions::new());
    assert!(
        matches!(
            refusal,
            Err(Error::Bus {
                chip: "FPGA",
                access: Access::Read,
                reg: 0,
                source: BusError::Timeout,
                ..
            })
        ),
        "libusdr would carry on; rsl-lsdr reports the read: {refusal:?}"
    );
    assert!(
        bus.sim.board().trace().is_empty(),
        "nothing follows the read"
    );
}

#[test]
fn close_attempts_every_write_and_reports_the_first_failure() {
    let bus = Faulty::new(Variant::DualAdcClockGen);
    let device = Device::open(bus.clone(), OpenOptions::new()).expect("an LSDr opens");
    let opened = bus.sim.board().trace().len();
    // The Si549 gate and the first ADC's power-down fail.
    bus.fail_next(&[0, 2]);
    let outcome = device.close();
    assert!(
        matches!(
            outcome,
            Err(Error::Bus {
                chip: "Si549",
                access: Access::Write,
                reg: 17,
                ..
            })
        ),
        "the first failure is reported: {outcome:?}"
    );
    let board = bus.sim.board();
    let pmic = I2cAddress::new(0, 0x48);
    assert_eq!(
        board.trace()[opened..],
        [
            Op::I2c {
                addr: pmic,
                write: vec![0x20, 0x20],
                read: vec![],
            },
            Op::Spi {
                bus: 1,
                out: 0x08e0,
                read: 0,
            },
        ],
        "the writes after each failure still run: LDO4 off, then the second ADC"
    );
}
