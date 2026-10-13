//! `rsl_usdr::dsp::sincos`'s test-signal conversions, tones and chirps against libusdr's
//! `usdr_dm_create` tool, compiled whole: its phase and gain conversions, its producer
//! threads' packets (the tool's own threads filling its own ring), and its TX loop sending
//! them to the simulated board's TX core.
//!
//! The tool's option handling is not compared: its `-g` clamp, its chirp-step rewrite and
//! its option defaults are command-line policy rsl-usdr does not port (PORTING.md), so
//! these tests hand both sides the same thread inputs. Phases and chirp steps wrap within a
//! few samples here, which is signed overflow in C; the oracle builds the tool and its
//! kernels with `-fwrapv`, so both wrap.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::num::{NonZeroI32, NonZeroU32};
use std::time::Duration;

use num_complex::Complex;
use rsl_usdr::dsp::sincos::{Amplitude, Chirp, Tone, ToneF32, gain_from_dbfs, phase_from_turns};
use rsl_usdr::{Device, HostFormat, StreamTime, StreamsConfig, SyncMode, TxConfig, WireFormat};
use rsl_usdr_oracle::Oracle;
use rsl_usdr_oracle::dm_create::{self, Producer, ThreadInput, generate};
use rsl_usdr_sim::{BoardRevision, SimBoard};
use sim_bus::SimBus;

/// The tool's `ci16` full scale (`MAX_TXGEN_CI16_AMPL`).
const FULL_SCALE: i16 = 32760;
/// Packets taken from each producer.
const PACKETS: usize = 3;
/// Samples per packet: one, a few, and the tool's default.
const PACKET_SAMPLES: [u32; 3] = [1, 7, 4096];

/// Deterministic xorshift64.
struct Rng(u64);

impl Rng {
    /// The next 64 random bits.
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A value in `[0, 1)`.
    #[expect(clippy::cast_precision_loss, reason = "53 random bits")]
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1_u64 << 53) as f64
    }
}

/// The tool's amplitude for `dbfs`: its sine inverted, at its full scale.
fn tool_amplitude(dbfs: f32) -> Amplitude {
    Amplitude {
        gain: gain_from_dbfs(dbfs, FULL_SCALE).expect("a gain within int16_t"),
        invert_sin: true,
        invert_cos: false,
    }
}

/// `ci16` samples as the tool's buffer holds them.
fn ci16_bytes(samples: &[Complex<i16>]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|s| [s.re.to_le_bytes(), s.im.to_le_bytes()].concat())
        .collect()
}

/// `cf32` samples as the tool's buffer holds them.
fn cf32_bytes(samples: &[Complex<f32>]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|s| [s.re.to_le_bytes(), s.im.to_le_bytes()].concat())
        .collect()
}

/// `PACKETS` packets of `samples` from `fill`, as bytes.
fn packets(samples: u32, mut fill: impl FnMut(usize) -> Vec<u8>) -> Vec<Vec<u8>> {
    let samples = usize::try_from(samples).expect("fits");
    (0..PACKETS).map(|_| fill(samples)).collect()
}

/// The tool's producer's packets, checked unflagged.
fn tool_packets(producer: &Producer<'_>, payload: usize) -> Vec<Vec<u8>> {
    generate(producer, payload, PACKETS)
        .into_iter()
        .map(|packet| {
            assert_eq!(packet.flags, 0, "a generator flags nothing");
            packet.bytes
        })
        .collect()
}

/// Every phase the tool can convert, edges and a random sweep, converts to the same
/// 32-bit phase.
#[test]
fn phases_from_turns_match_the_tools_conversion() {
    let mut rng = Rng(0x5eed_0001);
    let edges = [
        -0.499_999_999_9,
        -0.25,
        0.0,
        1.0 / 3.0,
        0.499_999_999_9,
        0.5,
        0.75,
        1.0,
        1.0 + 1e-10,
    ];
    let random = (0..100_000).map(|_| -0.5 + 1.5 * rng.unit());
    for turns in edges.into_iter().chain(random) {
        let Some(phase) = phase_from_turns(turns) else {
            assert!(turns <= -0.5, "{turns} turns are within the C's range");
            continue;
        };
        // SAFETY: `phase_from_turns` answered, so the C conversion is defined here.
        let expected = unsafe { dm_create::phase(turns) };
        assert_eq!(phase, expected, "{turns} turns");
    }
}

/// Every attenuation from 0 to -150 dBFS in thousandths of a dB, and the few thousandths
/// above 0 that still fit, give the same gain.
#[test]
fn gains_from_dbfs_match_the_tools_conversion() {
    let thousandths = (-150_000..=3).map(|k: i32| {
        #[expect(clippy::cast_precision_loss, reason = "small integers")]
        let dbfs = k as f32 / 1000.0;
        dbfs
    });
    let mut compared = 0;
    for dbfs in thousandths {
        let Some(gain) = gain_from_dbfs(dbfs, FULL_SCALE) else {
            assert!(dbfs > 0.0, "{dbfs} dBFS fits int16_t");
            continue;
        };
        // SAFETY: `gain_from_dbfs` answered, so the C conversion is defined here.
        let expected = unsafe { dm_create::gain(dbfs) };
        assert_eq!(gain, expected, "{dbfs} dBFS");
        compared += 1;
    }
    assert!(compared > 150_000, "the sweep reached past 0 dBFS");
}

/// The tool's tone: its four default channels and a few more phases, steps and gains,
/// over three packets so the phase carries.
#[test]
fn tones_match_the_tools_generator() {
    // (start turns, step turns, dBFS): the tool's channels 0 to 3, then others.
    let tones = [
        (0.0, 1.0 / 3.0, 0.0),
        (0.5, 0.02, 0.0),
        (0.25, 0.03, 0.0),
        (0.125, 0.04, 0.0),
        (0.9, -0.1, -6.0),
        (-0.3, 0.499_9, -40.5),
        (0.6, 0.75, -0.002),
    ];
    for (start, step, dbfs) in tones {
        for samples in PACKET_SAMPLES {
            let input = ThreadInput {
                samplerate: 50_000_000,
                samples,
                gain: dbfs,
                start_phase: start,
                delta_phase: step,
                ..ThreadInput::default()
            };
            let expected = tool_packets(
                &Producer::Ci16(input),
                4 * usize::try_from(samples).expect("fits"),
            );
            let mut tone = Tone::new(
                phase_from_turns(start).expect("in range"),
                phase_from_turns(step).expect("in range"),
                tool_amplitude(dbfs),
            );
            let actual = packets(samples, |n| {
                let mut out = vec![Complex::new(0, 0); n];
                tone.fill_complex(&mut out);
                ci16_bytes(&out)
            });
            assert!(
                expected == actual,
                "tone {start} + {step} turns at {dbfs} dBFS, {samples}-sample packets"
            );
        }
    }
}

/// The tool's chirp: its four default channels at its default 50 MS/s, and short sweeps
/// that restart within a packet, both ways.
#[test]
fn chirps_match_the_tools_generator() {
    // (start turns, steps, low Hz, high Hz, sample rate, dBFS): the tool's channels 0 to 3,
    // then others.
    let chirps = [
        (0.0, -100_007, -333_333.0, 333_333.0, 50_000_000, 0.0),
        (0.5, 100_007, -666_666.0, 666_666.0, 50_000_000, 0.0),
        (0.25, -1_000_007, -1e6, 1e6, 50_000_000, 0.0),
        (0.125, 1_000_007, -10e6, 10e6, 50_000_000, 0.0),
        (0.3, 7, -1e6, 1.5e6, 4_000_000, -3.0),
        (0.7, -15, 100_000.0, 1_900_000.0, 4_000_000, -12.25),
    ];
    for (start, steps, low, high, rate, dbfs) in chirps {
        for samples in PACKET_SAMPLES {
            let input = ThreadInput {
                samplerate: rate,
                samples,
                gain: dbfs,
                start_phase: start,
                chirp_freq0: low,
                chirp_freq1: high,
                chirp_steps: steps,
                ..ThreadInput::default()
            };
            let expected = tool_packets(
                &Producer::Ci16(input),
                4 * usize::try_from(samples).expect("fits"),
            );
            let mut chirp = Chirp::sweep(
                start,
                low,
                high,
                NonZeroU32::new(rate).expect("nonzero"),
                NonZeroI32::new(steps).expect("nonzero"),
                tool_amplitude(dbfs),
            )
            .expect("a valid sweep");
            let actual = packets(samples, |n| {
                let mut out = vec![Complex::new(0, 0); n];
                chirp.fill_complex(&mut out);
                ci16_bytes(&out)
            });
            assert!(
                expected == actual,
                "chirp from {start} turns, {steps} steps, {low} to {high} Hz at {rate}, {samples}-sample packets"
            );
        }
    }
}

/// The tool's `cf32` tone with amplitudes as it takes them (its `gain` as it stands), over
/// steps that wrap the phase past a turn both ways.
#[test]
fn float_tones_match_the_tools_generator() {
    // (start turns, step turns, amplitude).
    let tones = [
        (0.0, 1.0 / 3.0, 1.0),
        (0.5, 0.02, 0.25),
        (-0.9, -0.3, -0.5),
        (0.95, 0.75, 0.999),
    ];
    for (start, step, amplitude) in tones {
        for samples in PACKET_SAMPLES {
            let input = ThreadInput {
                samplerate: 50_000_000,
                samples,
                gain: amplitude,
                start_phase: start,
                delta_phase: step,
                ..ThreadInput::default()
            };
            let expected = tool_packets(
                &Producer::Cf32(input),
                8 * usize::try_from(samples).expect("fits"),
            );
            let mut tone = ToneF32::new(start, step, amplitude);
            let actual = packets(samples, |n| {
                let mut out = vec![Complex::new(0.0, 0.0); n];
                tone.fill(&mut out);
                cf32_bytes(&out)
            });
            assert!(
                expected == actual,
                "float tone {start} + {step} turns at {amplitude}, {samples}-sample packets"
            );
        }
    }
}

/// Samples per TX packet in the end-to-end runs.
const TX_SAMPLES: u32 = 1024;
/// Iterations of the tool's TX loop in the end-to-end runs.
const ITERATIONS: usize = 6;

/// The settings both end-to-end runs generate from: a tone, or a chirp.
fn tx_input(chirp: bool) -> ThreadInput {
    ThreadInput {
        samplerate: 20_000_000,
        samples: TX_SAMPLES,
        gain: -1.5,
        start_phase: 0.125,
        delta_phase: 0.04,
        chirp_freq0: if chirp { -1e6 } else { 0.0 },
        chirp_freq1: if chirp { 1e6 } else { 0.0 },
        chirp_steps: if chirp { 1023 } else { 0 },
    }
}

/// rsl-usdr's version of `tx_input(chirp)`'s generator.
enum Signal {
    /// A tone.
    Tone(Tone),
    /// A chirp.
    Chirp(Chirp),
}

impl Signal {
    /// Built from the same settings as the tool's thread.
    fn new(input: ThreadInput) -> Self {
        let amplitude = tool_amplitude(input.gain);
        match NonZeroI32::new(input.chirp_steps) {
            None => Self::Tone(Tone::new(
                phase_from_turns(input.start_phase).expect("in range"),
                phase_from_turns(input.delta_phase).expect("in range"),
                amplitude,
            )),
            Some(steps) => Self::Chirp(
                Chirp::sweep(
                    input.start_phase,
                    input.chirp_freq0,
                    input.chirp_freq1,
                    NonZeroU32::new(input.samplerate).expect("nonzero"),
                    steps,
                    amplitude,
                )
                .expect("a valid sweep"),
            ),
        }
    }

    /// The next packet.
    fn fill(&mut self, out: &mut [Complex<i16>]) {
        match self {
            Self::Tone(tone) => tone.fill_complex(out),
            Self::Chirp(chirp) => chirp.fill_complex(out),
        }
    }
}

/// The board the end-to-end runs use.
fn board() -> SimBoard {
    SimBoard::new(BoardRevision::Rev3)
}

/// The tool's TX loop on libusdr: the stream set up as `tx_stream_parity` sets it up, then
/// the loop's operations and what the TX core played.
fn tool_transmits(chirp: bool, nots: bool) -> (Vec<rsl_usdr_sim::Op>, Vec<rsl_usdr_sim::TxPacket>) {
    let mut oracle = Oracle::open(board()).expect("libusdr opens");
    oracle.set_rx_rate(20_000_000).expect("sets the rate");
    oracle.set_tx_frequency(1_500_000_000).expect("tunes TX");
    oracle
        .create_tx_stream_as(c"ci16", TX_SAMPLES)
        .expect("libusdr creates TX");
    oracle.start_tx_stream().expect("libusdr starts TX");
    oracle.sync_streams(c"none").expect("libusdr syncs");
    let start = oracle.trace_len();
    let sent = oracle
        .transmit_generated(&Producer::Ci16(tx_input(chirp)), ITERATIONS, nots)
        .expect("the tool's loop runs");
    let end = oracle.trace_len();
    assert_eq!(sent.samples, [TX_SAMPLES; ITERATIONS], "whole packets");
    assert!(!sent.stopped, "a generator never ends the loop");
    let board = oracle.close();
    (
        board.trace()[start..end].to_vec(),
        board.tx_packets().to_vec(),
    )
}

/// rsl-usdr generating the same packets and sending them as the tool times them: the first
/// one packet into the stream, each next right after, or all "now".
fn rsl_transmits(chirp: bool, nots: bool) -> (Vec<rsl_usdr_sim::Op>, Vec<rsl_usdr_sim::TxPacket>) {
    let sim = SimBus::new(board());
    let mut device = Device::with_bus(sim.clone()).expect("rsl-usdr powers up");
    device
        .set_rx_sample_rate(20_000_000)
        .expect("sets the rate");
    device.set_tx_frequency(1_500_000_000).expect("tunes TX");
    let config = StreamsConfig::tx(TxConfig::new(
        TX_SAMPLES,
        HostFormat::Ci16,
        WireFormat::Ci16,
    ))
    .with_sync(SyncMode::None);
    device.create_streams(config).expect("rsl-usdr creates");
    device.start_streams().expect("rsl-usdr starts");
    let start = sim.board().trace().len();
    let mut signal = Signal::new(tx_input(chirp));
    let mut packet = vec![Complex::new(0, 0); usize::try_from(TX_SAMPLES).expect("fits")];
    let mut time = u64::from(TX_SAMPLES);
    for _ in 0..ITERATIONS {
        signal.fill(&mut packet);
        let at = (!nots).then_some(StreamTime::from_samples(time));
        device
            .send_with_stat(&[&packet], at, Duration::from_millis(100))
            .expect("rsl-usdr sends");
        time += u64::from(TX_SAMPLES);
    }
    let board = sim.board();
    (board.trace()[start..].to_vec(), board.tx_packets().to_vec())
}

/// The tool's TX loop and rsl-usdr's sends of the same signal, timed as the tool times
/// them or "now": the same operations, and the TX core plays the same packets at the same
/// times.
#[test]
fn generated_packets_reach_the_tx_core_as_the_tools_loop_sends_them() {
    for (chirp, nots) in [(false, false), (true, true)] {
        let (expected_ops, expected_played) = tool_transmits(chirp, nots);
        let (actual_ops, actual_played) = rsl_transmits(chirp, nots);
        assert_eq!(
            expected_ops, actual_ops,
            "chirp {chirp}, now {nots}: operations"
        );
        assert!(
            expected_played == actual_played,
            "chirp {chirp}, now {nots}: the TX core's packets"
        );
        assert_eq!(actual_played.len(), ITERATIONS);
    }
}
