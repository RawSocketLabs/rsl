//! The V5's sample rate, tuning and gain against the registers librtlsdr left in every
//! sweep session (`RTL-OBS-11` to `RTL-OBS-18`), and the tune's traffic against a capture.

#[path = "common/replay_port.rs"]
mod replay_port;
#[path = "common/wire.rs"]
mod wire;

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use replay_port::ReplayPort;
use rtlsdr::control::{BusError, ControlPort, Setup};
use rtlsdr::r820t2::registers::I2C_ADDRESS;
use rtlsdr::rtl2832u::{GATE_CLOSED, GATE_OPEN, I2C_BLOCK, Location, WRITE};
use rtlsdr::v5::{GAIN_LABELS_TENTH_DB, Gain, GainStep};
use rtlsdr::{Device, Error};
use rtlsdr_sim::Trace;

/// What a [`Board`] has been sent.
#[derive(Default)]
struct Seen {
    /// Tuner registers by address.
    tuner: HashMap<u8, u8>,
    /// Paged-block page 1 registers by address.
    page1: HashMap<u8, u8>,
    /// Whether the gate is open.
    gate: bool,
    /// Every tuner write, register then values.
    writes: Vec<Vec<u8>>,
    /// Every page 1 write: register, values.
    page1_writes: Vec<(u8, Vec<u8>)>,
}

/// A V5 as far as the driver's writes go: probes stall, R0 is the R820T2's, the PLL
/// reports `locked`; it keeps what it is sent.
#[derive(Clone)]
struct Board {
    /// What it has been sent.
    seen: Arc<Mutex<Seen>>,
    /// Whether R2 reports lock.
    locked: bool,
}

impl Board {
    /// A board whose PLL reports `locked`.
    fn new(locked: bool) -> Self {
        Self {
            seen: Arc::default(),
            locked,
        }
    }

    /// Whether `setup` is an I2C access, and if so whether to the tuner.
    fn i2c(setup: Setup) -> Option<bool> {
        let tuner = Location::i2c(I2C_ADDRESS).expect("7-bit").value();
        ((setup.index & !WRITE) >> 8 == u16::from(I2C_BLOCK)).then_some(setup.value == tuner)
    }
}

impl ControlPort for Board {
    fn control_in(
        &mut self,
        setup: Setup,
        data: &mut [u8],
        _: Duration,
    ) -> Result<usize, BusError> {
        data.fill(0);
        match Board::i2c(setup) {
            Some(false) => return Err(BusError::Timeout),
            Some(true) => {
                // R0 0x96 and R2's lock bit 0x40, LSB first on the wire (R820-DS-04).
                let wire = [0x69, 0x00, if self.locked { 0x02 } else { 0x00 }];
                let n = data.len().min(wire.len());
                data[..n].copy_from_slice(&wire[..n]);
            }
            None => {}
        }
        Ok(data.len())
    }

    fn control_out(&mut self, setup: Setup, data: &[u8], _: Duration) -> Result<(), BusError> {
        let mut seen = self.seen.lock().expect("unpoisoned");
        match Board::i2c(setup) {
            Some(false) => return Err(BusError::Timeout),
            Some(true) => {
                assert!(seen.gate, "a tuner write with the gate closed: {data:02x?}");
                if let Some((&first, values)) = data.split_first() {
                    for (register, &value) in (first..).zip(values) {
                        seen.tuner.insert(register, value);
                    }
                }
                seen.writes.push(data.to_vec());
            }
            None if setup.index & 0xff0f == 0x0001 => {
                let first = u8::try_from(setup.value >> 8).expect("a byte");
                for (register, &value) in (first..).zip(data) {
                    seen.page1.insert(register, value);
                }
                seen.page1_writes.push((first, data.to_vec()));
                if first == 0x01 {
                    seen.gate = data[0] & (GATE_OPEN ^ GATE_CLOSED) != 0;
                }
            }
            None => {}
        }
        Ok(())
    }

    fn rx_start(&mut self, _: usize, _: NonZeroUsize) -> Result<(), BusError> {
        Err(BusError::Timeout)
    }

    fn rx_read(&mut self, _: &mut [u8], _: Duration) -> Result<usize, BusError> {
        Err(BusError::Timeout)
    }

    fn rx_stop(&mut self) -> Result<(), BusError> {
        Ok(())
    }

    fn sleep(&mut self, _: Duration) {}
}

/// `rtl_sdr -g`'s gain: `0` is automatic; otherwise librtlsdr's nearest labelled step.
fn gain(arg: &str) -> Gain {
    let (units, tenth) = arg
        .split_once('.')
        .expect("one decimal, as the fixture writes it");
    let tenths: u16 = format!("{units}{tenth}").parse().expect("tenths of a dB");
    if tenths == 0 {
        return Gain::Auto;
    }
    let step = (0..GAIN_LABELS_TENTH_DB.len())
        .min_by_key(|&k| GAIN_LABELS_TENTH_DB[k].abs_diff(tenths))
        .expect("steps");
    Gain::Manual(GainStep::new(u8::try_from(step).expect("small")).expect("a step"))
}

/// The tuner registers the fixture lists, in its order.
const TUNER: [u8; 15] = [
    0x05, 0x07, 0x0a, 0x0b, 0x0c, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x1a, 0x1b,
];

/// The page 1 registers the fixture lists, in its order.
const PAGE1: [u8; 7] = [0x19, 0x1a, 0x1b, 0x9f, 0xa0, 0xa1, 0xa2];

/// The registers the rate alone sets: R10, R11 and the page 1 ones.
const RATE_TUNER: [u8; 2] = [0x0a, 0x0b];

#[test]
fn every_sweep_session_leaves_the_registers_librtlsdr_left() {
    let text = include_str!("fixtures/gplobs-v5-sweep-state.txt");
    let (mut tuned, mut rates_refused, mut frequencies_refused) = (0, 0, 0);
    for line in text.lines().filter(|line| !line.starts_with('#')) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [session, f, s, g, ..] = fields[..] else {
            panic!("a session line: {line}");
        };
        let expected = |i: usize| -> Option<u8> { u8::from_str_radix(fields[4 + i], 16).ok() };
        let (frequency, rate) = (f.parse().expect("Hz"), s.parse().expect("S/s"));
        let board = Board::new(true);
        let mut device = Device::open(Box::new(board.clone())).expect("the V5 opens");
        match device.set_sample_rate(rate) {
            Err(Error::Rate(_)) => {
                // librtlsdr refused it too: it wrote no ratio.
                assert_eq!(expected(TUNER.len() + 3), None, "{session}: rate refused");
                rates_refused += 1;
                continue;
            }
            result => result.expect("the rate is set"),
        }
        let tune = device.tune(frequency);
        device.set_gain(gain(g)).expect("the gain is set");
        let seen = board.seen.lock().expect("unpoisoned");
        for (i, register) in PAGE1.iter().enumerate() {
            let at = TUNER.len() + i;
            assert_eq!(
                seen.page1.get(register).copied(),
                expected(at),
                "{session}: page 1 {register:#04x}"
            );
        }
        let checked: &[u8] = match tune {
            Err(Error::Frequency(_)) => {
                frequencies_refused += 1;
                &RATE_TUNER
            }
            result => {
                result.expect("tuned");
                tuned += 1;
                &TUNER
            }
        };
        for (i, register) in TUNER.iter().enumerate() {
            if checked.contains(register) {
                assert_eq!(
                    seen.tuner.get(register).copied(),
                    expected(i),
                    "{session}: R{register:#04x}"
                );
            }
        }
    }
    assert_eq!(
        (tuned, rates_refused, frequencies_refused),
        (726, 19, 24),
        "sessions by outcome"
    );
}

#[test]
fn a_tune_sends_what_rtl_sdr_sent_when_every_band_register_changed() {
    // GPLOBS-V5-0016 (433.92 MHz at 2.048 MS/s), its last tune: gate open, R23, R26, R27,
    // R0-R4 read, R16-R22, R0-R2 read (locked), R26, gate closed.
    let trace = Trace::parse(include_str!("fixtures/gplobs-v5-0016-tune.trace"))
        .expect("the fixture parses");
    let replay = ReplayPort::new(trace);
    let switch = Switch {
        board: Board::new(true),
        replay: replay.clone(),
        replaying: Arc::default(),
    };
    let mut device = Device::open(Box::new(switch.clone())).expect("the V5 opens");
    device.set_sample_rate(2_048_000).expect("the rate is set");
    *switch.replaying.lock().expect("unpoisoned") = true;
    device.tune(433_920_000).expect("tuned");
    replay
        .finish()
        .expect("every transfer the capture's, in order");
}

#[test]
fn a_rate_ends_with_the_writes_rtl_sdr_sent_after_its_ratio() {
    // GPLOBS-V5-0016, 2.048 MS/s: page 1 0x9f = 03 84, 0xa1 = 00 00, 0x3f = 00, 0x3e = 00,
    // then the gate register 0x14, 0x10 (RTL-OBS-12, RTL-OBS-14).
    let board = Board::new(true);
    let mut device = Device::open(Box::new(board.clone())).expect("the V5 opens");
    let before = board.seen.lock().expect("unpoisoned").page1_writes.len();
    device.set_sample_rate(2_048_000).expect("the rate is set");
    let seen = board.seen.lock().expect("unpoisoned");
    let tail: Vec<_> = seen.page1_writes[before..]
        .iter()
        .rev()
        .take(6)
        .rev()
        .cloned()
        .collect();
    let expected: Vec<(u8, Vec<u8>)> = vec![
        (0x9f, vec![0x03, 0x84]),
        (0xa1, vec![0x00, 0x00]),
        (0x3f, vec![0x00]),
        (0x3e, vec![0x00]),
        (0x01, vec![0x14]),
        (0x01, vec![0x10]),
    ];
    assert_eq!(tail, expected);
}

#[test]
fn a_new_rate_retunes_for_its_if_or_is_refused_unsent() {
    let board = Board::new(true);
    let mut device = Device::open(Box::new(board.clone())).expect("the V5 opens");
    device.set_sample_rate(2_048_000).expect("the rate is set");
    device.tune(100_000_000).expect("tuned");
    device.set_sample_rate(1_024_000).expect("the rate is set");
    // GPLOBS-V5-0024: 100 MHz at 1.024 MS/s leaves R20-R22 at ca 00 80 (IF 1.7 MHz).
    let pll = |seen: &Seen| [0x14, 0x15, 0x16].map(|r| seen.tuner[&r]);
    assert_eq!(
        pll(&board.seen.lock().expect("unpoisoned")),
        [0xca, 0x00, 0x80]
    );

    // 1.768 GHz is in range at 1.6 MS/s (IF 1.5 MHz) but not at 3.2 MS/s (IF 3.57 MHz).
    device.set_sample_rate(1_600_000).expect("the rate is set");
    device.tune(1_768_000_000).expect("tuned");
    let sent = board.seen.lock().expect("unpoisoned").page1_writes.len();
    assert!(matches!(
        device.set_sample_rate(3_200_000),
        Err(Error::Frequency(_))
    ));
    assert_eq!(
        board.seen.lock().expect("unpoisoned").page1_writes.len(),
        sent,
        "nothing sent"
    );
}

#[test]
fn an_unlocked_pll_is_retried_once_then_refused_with_the_gate_closed() {
    let board = Board::new(false);
    let mut device = Device::open(Box::new(board.clone())).expect("the V5 opens");
    device.set_sample_rate(2_048_000).expect("the rate is set");
    assert!(matches!(device.tune(100_000_000), Err(Error::Unlocked)));
    let seen = board.seen.lock().expect("unpoisoned");
    let last = seen.writes.iter().rev().find(|write| write.len() > 1);
    assert_eq!(last, Some(&vec![0x12, 0x60]), "R18 retried, no R26 after");
    assert!(!seen.gate);
}

#[test]
fn tuning_needs_a_rate_and_an_lo_in_range() {
    let mut device = Device::open(Box::new(Board::new(true))).expect("the V5 opens");
    assert!(matches!(device.tune(100_000_000), Err(Error::NoRate)));
    device.set_sample_rate(2_048_000).expect("the rate is set");
    // The LO is the frequency plus 1.625 MHz here; its kHz must be 27657 to 1769999.
    assert!(matches!(device.tune(26_031_000), Err(Error::Frequency(_))));
    assert!(device.tune(26_032_000).is_ok());
    assert!(device.tune(1_768_374_499).is_ok());
    assert!(matches!(
        device.tune(1_768_374_500),
        Err(Error::Frequency(_))
    ));
    assert!(matches!(
        device.set_sample_rate(225_000),
        Err(Error::Rate(_))
    ));
    assert!(matches!(
        device.set_sample_rate(3_200_001),
        Err(Error::Rate(_))
    ));
}

#[test]
fn gain_steps_follow_librtlsdrs_ladder() {
    let manual = |k| Gain::Manual(GainStep::new(k).expect("a step")).registers();
    assert_eq!(manual(0), [0x10, 0x60, 0x68]);
    assert_eq!(manual(1), [0x11, 0x60, 0x68]);
    assert_eq!(manual(27), [0x1e, 0x6d, 0x68]);
    assert_eq!(manual(28), [0x1f, 0x6e, 0x68], "the last step raises both");
    assert_eq!(GainStep::new(29), None);
    assert_eq!(Gain::Auto.registers(), [0x03, 0x75, 0x6b]);
}

/// A [`Board`] until `replaying` is set, then a [`ReplayPort`].
#[derive(Clone)]
struct Switch {
    /// Before the switch.
    board: Board,
    /// After it.
    replay: ReplayPort,
    /// Whether it has switched.
    replaying: Arc<Mutex<bool>>,
}

impl Switch {
    /// The port in use.
    fn port(&mut self) -> &mut dyn ControlPort {
        if *self.replaying.lock().expect("unpoisoned") {
            &mut self.replay
        } else {
            &mut self.board
        }
    }
}

impl ControlPort for Switch {
    fn control_in(
        &mut self,
        setup: Setup,
        data: &mut [u8],
        t: Duration,
    ) -> Result<usize, BusError> {
        self.port().control_in(setup, data, t)
    }

    fn control_out(&mut self, setup: Setup, data: &[u8], t: Duration) -> Result<(), BusError> {
        self.port().control_out(setup, data, t)
    }

    fn rx_start(&mut self, len: usize, depth: NonZeroUsize) -> Result<(), BusError> {
        self.port().rx_start(len, depth)
    }

    fn rx_read(&mut self, data: &mut [u8], t: Duration) -> Result<usize, BusError> {
        self.port().rx_read(data, t)
    }

    fn rx_stop(&mut self) -> Result<(), BusError> {
        self.port().rx_stop()
    }

    fn sleep(&mut self, d: Duration) {
        self.port().sleep(d);
    }
}
