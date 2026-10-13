//! `rsl_usdr::discipline` against libusdr's `usdr_dm_gpsdo` tool, compiled whole and run
//! on a simulated board with the `PCIe` front end's DAC: over scripted measurement words,
//! each poll's operations (the measurement read, then any DAC write) and the tool's
//! printed values (the measurement, the controller's output and correction, each DAC code
//! and the step to it, holdover) against rsl-usdr's read, step and write.
//!
//! The tool reads `/dm/sensor/freqpps`, which the uSDR lacks at the pin; the shim reads the
//! tool's commented-out alternative, `/dm/sdr/0/clkmeas` (GPI word 24), as rsl-usdr's
//! [`Device::clock_measurement`] does. That shows both loops act alike on the same words,
//! not what the uSDR's gateware reports there (see `rsl_usdr::discipline`).
//!
//! The tool keeps the code it last wrote in a function `static` that outlives a run, and
//! skips writing that code again. Its first run starts from 0, which no first correction
//! reaches; later runs in this process start from the last run's code, which the harness
//! carries into rsl-usdr's run as already written (`GpsdoRun::dac_before`).

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::num::NonZeroU32;
use std::time::Duration;

use rsl_usdr::Device;
use rsl_usdr::discipline::{Holdover, OscillatorDiscipline, Outcome, PpsMeasurement};
use rsl_usdr_oracle::gpsdo::{Event, Setup, run_gpsdo};
use rsl_usdr_sim::{BoardRevision, FrontEnd, Op, SimBoard};
use sim_bus::SimBus;

/// The frequency both loops expect: the tool's default sample rate.
const EXPECTED: u32 = 4_000_000;
/// Polls per second: the tool's 100 ms timer.
const POLLS_PER_PULSE: usize = 10;

/// What one side did: each measurement read's operations (the initial read first) and the
/// loop's events, with read marks zeroed (the two traces differ in length before them).
#[derive(Debug)]
struct Side {
    /// Each read's operations, up to the next read.
    windows: Vec<Vec<Op>>,
    /// The loop's events after the set-up.
    events: Vec<Event>,
}

/// The board both sides use.
fn board() -> SimBoard {
    SimBoard::new(BoardRevision::Rev3).with_front_end(FrontEnd::Pciefe)
}

/// A measurement word: pulse counter `pulses` (mod 16) and `count`.
fn word(pulses: usize, count: u32) -> u32 {
    u32::try_from(pulses % 16).expect("four bits") << 28 | count
}

/// A script: the initial read, then each pulse's count held for a second of polls, the
/// counter moving at each.
fn pulses(counts: &[u32]) -> Vec<u32> {
    let mut words = vec![word(0, EXPECTED)];
    for (k, &count) in counts.iter().enumerate() {
        words.extend([word(k + 1, count)].repeat(POLLS_PER_PULSE));
    }
    words
}

/// The tool on libusdr: its set-up calls, then its loop.
fn tool(words: &[u32]) -> (Vec<Setup>, Side, u64) {
    let run = run_gpsdo(board(), &["-r", &EXPECTED.to_string()], words);
    assert_eq!(run.status, 0, "the tool ends at the end of the script");
    let trace = run.board.trace();
    let mut setup = Vec::new();
    let mut marks = Vec::new();
    let mut events = Vec::new();
    for event in run.events {
        match event {
            Event::Setup(call) => setup.push(call),
            Event::Read(mark) => {
                marks.push(mark);
                events.push(Event::Read(0));
            }
            event => events.push(event),
        }
    }
    // The read one past the script fails, the tool reports it and stops.
    assert_eq!(
        events.pop(),
        Some(Event::Stderr),
        "the failed read ends the loop"
    );
    assert_eq!(events.pop(), Some(Event::Read(0)));
    assert_eq!(
        marks.len(),
        words.len() + 1,
        "one read per word, and the failed one"
    );
    let windows = marks
        .windows(2)
        .map(|pair| trace[pair[0]..pair[1]].to_vec())
        .collect();
    (setup, Side { windows, events }, run.dac_before)
}

/// rsl-usdr: the same reads through the device, steps through the discipline machine and
/// writes to the DAC, recording the events the tool prints for them. A first write of
/// `already_written` is confirmed without writing, as the tool's leftover `static` skips
/// it.
fn rsl(words: &[u32], already_written: u64) -> Side {
    let sim = SimBus::new(board());
    let mut device = Device::with_bus(sim.clone()).expect("rsl-usdr powers up");
    let expected = NonZeroU32::new(EXPECTED).expect("nonzero");
    let mut windows = Vec::new();
    let mut events = Vec::new();
    let mut discipline = None;
    let mut first_write = true;
    for (poll, &word) in words.iter().enumerate() {
        let start = sim.board().trace().len();
        sim.board().set_clock_measurement(word);
        let read = device.clock_measurement().expect("reads the measurement");
        events.push(Event::Read(0));
        let measurement = PpsMeasurement::from_word(read);
        let now = Duration::from_millis(100 * u64::try_from(poll).expect("small"));
        let Some(discipline) = discipline.as_mut() else {
            discipline = Some(OscillatorDiscipline::new(expected, measurement, now));
            windows.push(sim.board().trace()[start..].to_vec());
            continue;
        };
        let offset = discipline.dac_code();
        let step = discipline.measure(measurement, now);
        let pps = |error: f64| Event::Pps {
            word: u64::from(read),
            count: i32::from(measurement.pulses),
            measured: f64::from(measurement.count),
            error_ppm: error * 1e6,
        };
        match step.outcome {
            Outcome::NoPulse => {}
            Outcome::Rejected { error } => events.extend([pps(error), Event::Skip]),
            Outcome::Corrected(correction) => {
                events.push(pps(correction.error));
                events.push(Event::Pout {
                    output: correction.output,
                    correction_hz: correction.correction_hz,
                    steps: correction.steps,
                });
                if correction.write {
                    let code = correction.dac;
                    let skipped = first_write && u64::from(code) == already_written;
                    first_write = false;
                    if !skipped {
                        events.push(Event::Dac {
                            code: code.into(),
                            bits: 16,
                            offset: offset.into(),
                            step: correction.steps.clamp(-2000.0, 2000.0),
                        });
                        device.set_oscillator_dac(code).expect("writes the DAC");
                        events.push(Event::DacWrite {
                            code: code.into(),
                            result: 0,
                        });
                    }
                    discipline.dac_written(code);
                }
            }
        }
        match step.holdover {
            Some(Holdover::Entered) => events.push(Event::Holdover(2.0)),
            Some(Holdover::Left) => events.push(Event::Found),
            None => {}
        }
        windows.push(sim.board().trace()[start..].to_vec());
    }
    Side { windows, events }
}

/// Both sides over `words`, compared poll for poll; returns them.
fn assert_same(words: &[u32]) -> (Side, Side) {
    let (_, expected, dac_before) = tool(words);
    let actual = rsl(words, dac_before);
    assert_eq!(
        expected.windows.len(),
        actual.windows.len(),
        "one window per read"
    );
    for (poll, (expected, actual)) in expected.windows.iter().zip(&actual.windows).enumerate() {
        assert_eq!(expected, actual, "poll {poll}'s operations");
    }
    assert_eq!(expected.events, actual.events, "the loop's events");
    (expected, actual)
}

/// The DAC writes among `events`.
fn dac_writes(events: &[Event]) -> Vec<u64> {
    events
        .iter()
        .filter_map(|event| match *event {
            Event::DacWrite { code, .. } => Some(code),
            _ => None,
        })
        .collect()
}

/// The tool sets the board up before its loop: what a caller of the machine does first.
#[test]
fn the_tool_sets_up_an_external_reference_the_rate_and_an_rx_stream_first() {
    let (setup, _, _) = tool(&pulses(&[]));
    assert_eq!(
        setup,
        [
            Setup::ReferenceFrequency(25_000_000),
            Setup::ExternalReference(true),
            Setup::Rate(EXPECTED.into()),
            Setup::RxStream(4096),
            Setup::SyncAny(1),
            Setup::Op(0),
        ]
    );
}

/// Counts a few hertz either side of the expected frequency, where the controller's
/// steps stay within their limit, and some far enough out that they reach it: the same
/// reads, controller values and DAC codes.
#[test]
fn corrections_match_the_tools_loop() {
    let counts = [
        EXPECTED + 1,
        EXPECTED + 2,
        EXPECTED,
        EXPECTED - 1,
        EXPECTED - 3,
        EXPECTED,
        EXPECTED,
        EXPECTED + 40,
        EXPECTED - 1000,
        EXPECTED + 1,
        EXPECTED,
    ];
    let (expected, _) = assert_same(&pulses(&counts));
    assert!(
        dac_writes(&expected.events).len() >= 8,
        "most pulses move the DAC"
    );
}

/// A count more than 2% above the expected frequency is skipped at every poll until the
/// same pulse reads a count within it.
#[test]
fn rejections_above_two_percent_match_the_tools_loop() {
    let mut words = pulses(&[EXPECTED + 1]);
    words.extend([word(2, EXPECTED + EXPECTED / 25)].repeat(4));
    words.extend([word(2, EXPECTED - 2)].repeat(POLLS_PER_PULSE));
    let (expected, _) = assert_same(&words);
    let skips = expected
        .events
        .iter()
        .filter(|e| **e == Event::Skip)
        .count();
    assert_eq!(skips, 4, "one skip per poll of the high count");
}

/// Three seconds without a pulse enters holdover; the next pulse leaves it. The pulse
/// counter also jumps by more than one and wraps past 15.
#[test]
fn holdover_and_counter_wraps_match_the_tools_loop() {
    let mut words = pulses(&[EXPECTED + 1, EXPECTED - 1]);
    words.extend([word(2, EXPECTED - 1)].repeat(3 * POLLS_PER_PULSE));
    words.extend([word(7, EXPECTED + 2)].repeat(POLLS_PER_PULSE));
    words.extend([word(16 + 1, EXPECTED)].repeat(POLLS_PER_PULSE));
    let (expected, _) = assert_same(&words);
    assert!(expected.events.contains(&Event::Holdover(2.0)));
    assert!(expected.events.contains(&Event::Found));
}

/// Divergence, scoped to this scenario: a count far below the expected frequency (here 0,
/// as from a counter that has not counted) is acted on by the tool, which rejects only
/// counts above it, and rejected by rsl-usdr. Up to it both agree; then the tool steps the
/// DAC up its limit while rsl-usdr skips.
#[test]
fn a_count_far_below_is_rejected_where_the_tool_rails_the_dac() {
    let before = pulses(&[EXPECTED + 1]);
    let mut words = before.clone();
    words.extend([word(2, 0)].repeat(POLLS_PER_PULSE));
    let (_, tool_run, dac_before) = tool(&words);
    let rsl_run = rsl(&words, dac_before);

    let agreed = before.len();
    assert_eq!(tool_run.windows[..agreed], rsl_run.windows[..agreed]);
    let tool_writes = dac_writes(&tool_run.events);
    let rsl_writes = dac_writes(&rsl_run.events);
    assert_eq!(
        rsl_writes.len(),
        1,
        "rsl-usdr writes for the first pulse only"
    );
    assert_eq!(
        tool_writes.len(),
        2,
        "the tool writes for the zero count too"
    );
    assert_eq!(tool_writes[1], rsl_writes[0] + 2000, "a full step up");
    let rsl_skips = rsl_run.events.iter().filter(|e| **e == Event::Skip).count();
    assert_eq!(
        rsl_skips, POLLS_PER_PULSE,
        "rsl-usdr skips the zero count at every poll"
    );
}

/// Divergence, scoped to this scenario: a count persistently above the expected frequency
/// (by less than 2%) walks the DAC down 2000 codes a pulse. At the bottom the tool rounds
/// the code below 0 and converts it to `uint32_t`, which wraps it to full scale; rsl-usdr
/// holds it at 0. Up to there both agree.
#[test]
fn the_dac_holds_at_zero_where_the_tool_wraps_it_to_full_scale() {
    let words = pulses(&[EXPECTED + 1000; 18]);
    let (_, tool_run, dac_before) = tool(&words);
    let rsl_run = rsl(&words, dac_before);

    // Sixteen steps of 2000 from 32767 reach 767; the seventeenth pulse goes below 0.
    let agreed = 1 + 16 * POLLS_PER_PULSE;
    assert_eq!(tool_run.windows[..agreed], rsl_run.windows[..agreed]);
    let tool_writes = dac_writes(&tool_run.events);
    let rsl_writes = dac_writes(&rsl_run.events);
    assert_eq!(tool_writes[..16], rsl_writes[..16]);
    assert_eq!(rsl_writes[15], 767);
    assert_eq!(tool_writes[16], 65_535, "the tool wraps to full scale");
    assert_eq!(rsl_writes[16], 0, "rsl-usdr stops at 0");
    assert_eq!(rsl_writes.len(), 17, "and stays there");
}
