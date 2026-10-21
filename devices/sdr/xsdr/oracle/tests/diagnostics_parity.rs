//! xsdr's link debug controls and board readings against libusdr's, operation for
//! operation and value for value, on simulated XSDRs and XTRXs after a sample rate:
//! `/dm/sdr/0/phy_rx_lfsr`, `.../phy_tx_lfsr`, `.../phy_tx_iqsel`, `.../phy_rx_dly`,
//! `.../dumprx`, `/debug/clk_info`, `/dm/sensor/freqpps`, `/dm/sdr/0/usbclk`,
//! `/dm/sdr/0/vio` and `/dm/sdr/0/rfe/pwrdc`.
//!
//! No divergence is whitelisted. libusdr packs the LFSR counters into one number, each
//! saturated to 16 bits; the test packs xsdr's whole counters the same way.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use sim_bus::SimBus;
use xsdr::{Device, Error, RxPhyDelay};
use xsdr_oracle::Oracle;
use xsdr_sim::{Op, SimBoard, Variant};

/// libusdr's call for one step: its value, if it reads one.
type LibusdrCall = Box<dyn Fn(&mut Oracle) -> Result<Option<u64>, i32>>;
/// xsdr's call for one step, likewise.
type XsdrCall = Box<dyn Fn(&mut Device, &SimBus) -> Result<Option<u64>, Error>>;

/// One control or reading applied to both drivers.
struct Step {
    /// What the step does, for failure messages.
    name: &'static str,
    /// The libusdr call.
    libusdr: LibusdrCall,
    /// The xsdr call.
    xsdr: XsdrCall,
}

/// A step setting libusdr's numeric `path` to `value`, xsdr's call alongside.
fn set(
    name: &'static str,
    path: &'static core::ffi::CStr,
    value: u64,
    xsdr: impl Fn(&mut Device) -> Result<(), Error> + 'static,
) -> Step {
    Step {
        name,
        // SAFETY: every path these steps set takes its value as a number.
        libusdr: Box::new(move |oracle| unsafe { oracle.set_uint(path, value) }.map(|()| None)),
        xsdr: Box::new(move |device, _| xsdr(device).map(|()| None)),
    }
}

/// libusdr's packing of the LFSR counters: each saturated to 16 bits, counter `n` at bit
/// `16 n`.
fn packed(counters: [u32; 4]) -> u64 {
    (0..)
        .zip(counters)
        .map(|(index, count)| u64::from(count.min(0xffff)) << (16 * index))
        .sum()
}

/// Panics at the first operation where `actual` leaves `expected`.
fn assert_same_ops(expected: &[Op], actual: &[Op], scenario: &str) {
    if let Some(at) =
        (0..expected.len().max(actual.len())).find(|&i| expected.get(i) != actual.get(i))
    {
        panic!(
            "{scenario}: xsdr diverges from libusdr at op {at}: libusdr {}, xsdr {}",
            expected.get(at).map_or("(end)".into(), Op::to_string),
            actual.get(at).map_or("(end)".into(), Op::to_string),
        );
    }
}

/// Opens fresh copies of `board` in both drivers, sets 10 MS/s, runs `steps` on each and
/// compares each step's operations, success and value.
fn assert_steps_match(board: impl Fn() -> SimBoard, steps: &[Step]) {
    let mut oracle = Oracle::open(board()).expect("libusdr opens");
    // SAFETY: the master rate takes its value as a number.
    unsafe { oracle.set_uint(c"/dm/rate/master", 10_000_000) }.expect("libusdr's rate");
    let mut expected = Vec::new();
    for step in steps {
        let start = oracle.trace_len();
        let value = (step.libusdr)(&mut oracle);
        expected.push((start..oracle.trace_len(), value));
    }
    let reference = oracle.close();

    let sim = SimBus::new(board());
    let mut device = Device::open(sim.clone()).expect("xsdr opens");
    device
        .set_sample_rate(10_000_000, 10_000_000)
        .expect("xsdr's rate");
    for (step, (window, value)) in steps.iter().zip(expected) {
        let start = sim.board().trace().len();
        let result = (step.xsdr)(&mut device, &sim);
        let actual: Vec<Op> = sim.board().trace()[start..].to_vec();
        assert_same_ops(&reference.trace()[window], &actual, step.name);
        match (value, result) {
            (Ok(theirs), Ok(ours)) => assert_eq!(ours, theirs, "{}: value", step.name),
            (Err(_), Err(_)) => {}
            (theirs, ours) => panic!("{}: libusdr {theirs:?}, xsdr {ours:?}", step.name),
        }
    }
    device.close().expect("xsdr closes");
}

/// Every control and reading, with values at and past their ranges.
fn steps() -> Vec<Step> {
    let mut steps = vec![
        set("LFSR checker on", c"/dm/sdr/0/phy_rx_lfsr", 1, |device| {
            device.set_rx_lfsr_checker(true)
        }),
        Step {
            name: "LFSR counters",
            libusdr: Box::new(|oracle| oracle.get_uint(c"/dm/sdr/0/phy_rx_lfsr").map(Some)),
            xsdr: Box::new(|device, _| device.rx_lfsr_errors().map(|counts| Some(packed(counts)))),
        },
        set("LFSR checker off", c"/dm/sdr/0/phy_rx_lfsr", 0, |device| {
            device.set_rx_lfsr_checker(false)
        }),
        set("IQ select", c"/dm/sdr/0/phy_tx_iqsel", 0b1010, |device| {
            device.set_tx_iq_select(0b1010)
        }),
        set("clock delay", c"/dm/sdr/0/phy_rx_dly", 5, |device| {
            device.set_rx_phy_delay(RxPhyDelay::Clock, 5)
        }),
        set(
            "frame delay",
            c"/dm/sdr/0/phy_rx_dly",
            1 << 8 | 31,
            |device| device.set_rx_phy_delay(RxPhyDelay::Frame, 31),
        ),
        set("capture", c"/dm/sdr/0/dumprx", 1, |device| {
            device.capture_rx().map(drop)
        }),
        set("clock readings", c"/debug/clk_info", 1, |device| {
            device.clock_readings().map(drop)
        }),
        Step {
            name: "1PPS ticks",
            libusdr: Box::new(|oracle| oracle.get_uint(c"/dm/sensor/freqpps").map(Some)),
            xsdr: Box::new(|device, _| device.pps_ticks().map(|ticks| Some(ticks.into()))),
        },
        set("USB clock on", c"/dm/sdr/0/usbclk", 1, |device| {
            device.set_usb_clock(true)
        }),
        set("USB clock off", c"/dm/sdr/0/usbclk", 0, |device| {
            device.set_usb_clock(false)
        }),
    ];
    for (run_it, lfsr) in [(false, false), (true, false), (false, true), (true, true)] {
        let value = u64::from(run_it) | u64::from(lfsr) << 1;
        steps.push(set(
            "TX generator",
            c"/dm/sdr/0/phy_tx_lfsr",
            value,
            move |device| device.set_tx_generator_pattern(run_it, lfsr),
        ));
    }
    for millivolts in [1000, 1700, 2000, 2500, 3400] {
        steps.push(set(
            "I/O supply",
            c"/dm/sdr/0/vio",
            millivolts.into(),
            move |device| device.set_io_voltage(millivolts).map(drop),
        ));
    }
    for (i, q) in [(0, 0), (1000, 0), (-300, 4000)] {
        let words = [i, q].map(|value: i32| u32::from_ne_bytes(value.to_ne_bytes()));
        steps.push(Step {
            name: "DC power",
            libusdr: Box::new(move |oracle| {
                oracle.board(|board| {
                    board.write_reg(7, words[0]);
                    board.write_reg(8, words[1]);
                });
                oracle.get_uint(c"/dm/sdr/0/rfe/pwrdc").map(Some)
            }),
            xsdr: Box::new(move |device, sim| {
                {
                    let mut board = sim.board();
                    board.write_reg(7, words[0]);
                    board.write_reg(8, words[1]);
                }
                device.rx_dc_power().map(|power| {
                    Some(power.map_or(1_000_000, |millidb| {
                        u64::from_ne_bytes(millidb.to_ne_bytes())
                    }))
                })
            }),
        });
    }
    steps
}

#[test]
fn every_debug_control_and_reading_matches_libusdr_on_an_xsdr() {
    assert_steps_match(
        || SimBoard::new(Variant::Xsdr).with_hwid(0x0030_0020),
        &steps(),
    );
}

#[test]
fn every_debug_control_and_reading_matches_libusdr_on_an_xtrx() {
    assert_steps_match(
        || SimBoard::new(Variant::Xtrx).with_hwid(0x002e_0020),
        &steps(),
    );
}

#[test]
fn a_delay_past_five_bits_is_refused_before_any_access() {
    let sim = SimBus::new(SimBoard::new(Variant::Xsdr));
    let mut device = Device::open(sim.clone()).expect("opens");
    let before = sim.board().trace().len();
    assert!(matches!(
        device.set_rx_phy_delay(RxPhyDelay::Clock, 32),
        Err(Error::InvalidDelay(32))
    ));
    assert_eq!(sim.board().trace().len(), before);
}
