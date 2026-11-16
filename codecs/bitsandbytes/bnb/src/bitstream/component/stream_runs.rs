//! Byte runs on the readers over `std::io::Read` against the per-byte loops they
//! replace: the same values, errors, partial fills and cursor, the same bytes taken
//! from the reader and the same ones retained, whatever the reader does on the way.
use crate::bitstream::stream::RUN_STEP;
use crate::{BitError, BitOrder, BufSource, ByteOrder, ErrorKind, Layout, Source, StreamBitReader};
use core::fmt;
use std::io;

/// A reader with a script: its bytes, the most it hands over per call, and what it
/// answers, once, when asked for the byte at an offset.
#[derive(Clone)]
struct Scripted {
    data: Vec<u8>,
    at: usize,
    chunk: usize,
    events: Vec<(usize, io::ErrorKind)>,
}

impl io::Read for Scripted {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if let Some(index) = self.events.iter().position(|event| event.0 == self.at) {
            return Err(self.events.remove(index).1.into());
        }
        // No `chunk`: a different amount at every offset.
        let chunk = if self.chunk == 0 {
            1 + self.at % 7
        } else {
            self.chunk
        };
        // Up to the next event and no further, so it fires however much is asked.
        let stop = self
            .events
            .iter()
            .map(|event| event.0)
            .filter(|&offset| offset > self.at)
            .min()
            .unwrap_or(self.data.len());
        let count = out.len().min(chunk).min(stop - self.at);
        out[..count].copy_from_slice(&self.data[self.at..self.at + count]);
        self.at += count;
        Ok(count)
    }
}

// What a reader has given up and has left to say: its bytes are the same in both.
impl fmt::Debug for Scripted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at {} events {:?}", self.at, self.events)
    }
}

fn noise(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "a test index, reduced first"
)]
fn below(state: &mut u64, bound: usize) -> usize {
    (noise(state) % bound as u64) as usize
}

fn pick<T: Copy>(state: &mut u64, from: &[T]) -> T {
    from[below(state, from.len())]
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    Bits(u32),
    Bytes(usize),
    Into(usize),
    Seek(usize),
    Compact,
}

#[derive(Debug, PartialEq)]
enum Outcome {
    Bits(Result<u128, BitError>),
    Bytes(Result<Vec<u8>, BitError>),
    Into(Result<(), BitError>, Vec<u8>),
    Seek(Result<(), BitError>),
    Compact,
}

impl Outcome {
    /// The error of a byte run, or that it gave all it was asked for.
    fn of_a_run(&self) -> Option<Result<(), &ErrorKind>> {
        match self {
            Outcome::Bytes(result) => Some(result.as_ref().map(|_bytes| ()).map_err(|e| &e.kind)),
            Outcome::Into(result, _) => Some(result.as_ref().copied().map_err(|e| &e.kind)),
            _ => None,
        }
    }
}

/// How a reader read bits before it asked for bytes together: the reference.
type BitsByByte<S> = fn(&mut S, u32) -> Result<u128, BitError>;

/// One step, through the reader's own reads or through `reference`, one byte of
/// a run at a time.
fn apply<S: Source>(
    s: &mut S,
    op: Op,
    reference: Option<BitsByByte<S>>,
    compact: fn(&mut S),
) -> Outcome {
    #[expect(clippy::cast_possible_truncation, reason = "eight bits were read")]
    let byte = |s: &mut S, bits: BitsByByte<S>| bits(s, 8).map(|value| value as u8);
    match (op, reference) {
        (Op::Bits(n), None) => Outcome::Bits(s.read_bits(n)),
        (Op::Bits(n), Some(bits)) => Outcome::Bits(bits(s, n)),
        (Op::Bytes(n), None) => Outcome::Bytes(s.read_bytes(n)),
        (Op::Bytes(n), Some(bits)) => Outcome::Bytes((0..n).map(|_| byte(s, bits)).collect()),
        (Op::Into(n), None) => {
            let mut buf = vec![0xEE; n];
            Outcome::Into(s.read_into(&mut buf), buf)
        }
        (Op::Into(n), Some(bits)) => {
            let mut buf = vec![0xEE; n];
            let result = buf
                .iter_mut()
                .try_for_each(|slot| byte(s, bits).map(|value| *slot = value));
            Outcome::Into(result, buf)
        }
        (Op::Seek(pos), _) => Outcome::Seek(s.seek_to_bit(pos)),
        (Op::Compact, _) => {
            compact(s);
            Outcome::Compact
        }
    }
}

/// Runs `program` on two readers built alike, one through its own reads and one
/// through `reference`, and holds them to the same outcome and the same state
/// after every step. Returns the outcome of each byte run.
fn in_step<S: Source + fmt::Debug>(
    make: impl Fn() -> S,
    program: &[Op],
    reference: BitsByByte<S>,
    compact: fn(&mut S),
    case: &str,
) -> Vec<Result<(), ErrorKind>> {
    let (mut bulk, mut plain) = (make(), make());
    let mut runs = Vec::new();
    for (step, &op) in program.iter().enumerate() {
        let outcome = apply(&mut bulk, op, None, compact);
        assert_eq!(
            outcome,
            apply(&mut plain, op, Some(reference), compact),
            "{case}, step {step}: {op:?}"
        );
        assert_eq!(
            format!("{bulk:?}"),
            format!("{plain:?}"),
            "{case}, after step {step}: {op:?}"
        );
        runs.extend(outcome.of_a_run().map(|run| run.map_err(Clone::clone)));
    }
    runs
}

const LAYOUTS: [Layout; 4] = [
    Layout {
        bit: BitOrder::Msb,
        byte: ByteOrder::Big,
    },
    Layout {
        bit: BitOrder::Msb,
        byte: ByteOrder::Little,
    },
    Layout {
        bit: BitOrder::Lsb,
        byte: ByteOrder::Big,
    },
    Layout {
        bit: BitOrder::Lsb,
        byte: ByteOrder::Little,
    },
];

/// A reader of random bytes with up to two things going wrong on the way.
fn script(state: &mut u64) -> Scripted {
    let len = match below(state, 16) {
        0 => 0,
        1 => 9_000,
        2..=5 => below(state, 12),
        _ => below(state, 300),
    };
    let data = (0..len).map(|_| noise(state).to_le_bytes()[0]).collect();
    let kinds = [
        io::ErrorKind::Interrupted,
        io::ErrorKind::Interrupted,
        io::ErrorKind::WouldBlock,
        io::ErrorKind::ConnectionReset,
        io::ErrorKind::UnexpectedEof,
    ];
    // In the long input, often where a step of a run ends.
    let events = (0..below(state, 3))
        .map(|_| {
            let at = if len == 9_000 && below(state, 2) == 0 {
                pick(state, &[4_095, 4_096, 4_097, 8_192])
            } else {
                below(state, len + 1)
            };
            (at, pick(state, &kinds))
        })
        .collect();
    Scripted {
        data,
        at: 0,
        chunk: pick(state, &[0, 0, 1, 2, 3, 7, 64, 5_000, usize::MAX]),
        events,
    }
}

/// Ten steps: mostly byte runs of every size, a count no input could back among
/// them, with bit reads between that leave the cursor inside a byte or on one.
fn program(state: &mut u64, len: usize) -> Vec<Op> {
    let size = |state: &mut u64| match below(state, 10) {
        0 => 0,
        1..=5 => below(state, 20),
        6 | 7 => below(state, 300),
        8 => below(state, 12_000),
        _ => usize::MAX >> below(state, 4),
    };
    (0..10)
        .map(|_| match below(state, 16) {
            0..=2 => Op::Bits(pick(
                state,
                &[0, 1, 3, 4, 5, 7, 8, 8, 16, 24, 32, 40, 64, 128, 129, 136],
            )),
            3..=8 => Op::Bytes(size(state)),
            9..=13 => Op::Into(size(state).min(12_000)),
            // To a byte as often as into one.
            14 => Op::Seek(below(state, len * 8 + 70) & !(7 * below(state, 2))),
            _ => Op::Compact,
        })
        .collect()
}

/// How often each way a run can end was met, so a differential that never left the
/// easy path fails instead of passing.
fn met(runs: &[Result<(), ErrorKind>], kind: impl Fn(&ErrorKind) -> bool) -> usize {
    runs.iter()
        .filter(|run| run.as_ref().err().is_some_and(&kind))
        .count()
}

#[test]
fn a_stream_readers_runs_are_its_byte_by_byte_reads() {
    let mut state = 0x9E37_79B9_7F4A_7C15;
    let mut runs = Vec::new();
    for case in 0..4_000 {
        let reader = script(&mut state);
        let layout = pick(&mut state, &LAYOUTS);
        // One case in eight starts where the position is about to stop counting.
        let near_the_end = (below(&mut state, 8) == 0).then(|| below(&mut state, 300));
        let program = program(&mut state, reader.data.len());
        let make = || {
            let mut subject = StreamBitReader::with_layout(reader.clone(), layout);
            if let Some(short) = near_the_end {
                subject.pos = usize::MAX - short;
            }
            subject
        };
        runs.extend(in_step(
            make,
            &program,
            StreamBitReader::read_bits_per_byte,
            |_| (),
            &format!("case {case}"),
        ));
    }
    assert!(runs.iter().filter(|run| run.is_ok()).count() > 2_000);
    assert!(met(&runs, |kind| matches!(kind, ErrorKind::Incomplete { .. })) > 500);
    assert!(met(&runs, |kind| matches!(kind, ErrorKind::Io(_))) > 100);
    assert!(met(&runs, |kind| matches!(kind, ErrorKind::PositionOverflow)) > 100);
}

#[test]
fn a_retaining_readers_runs_are_its_byte_by_byte_reads() {
    let mut state = 0xD1B5_4A32_D192_ED03;
    let mut runs = Vec::new();
    for case in 0..4_000 {
        let reader = script(&mut state);
        let layout = pick(&mut state, &LAYOUTS);
        let cap = pick(
            &mut state,
            &[
                0,
                1,
                3,
                16,
                64,
                4_095,
                4_096,
                4_097,
                5_000,
                64 * 1024,
                usize::MAX,
            ],
        );
        let near_the_end = (below(&mut state, 8) == 0).then(|| below(&mut state, 40));
        let program = program(&mut state, reader.data.len());
        let make = || {
            let mut subject = BufSource::with_capacity_and_layout(reader.clone(), cap, layout);
            if let Some(short) = near_the_end {
                // As after a long stream, compacted: `short` bytes still count.
                subject.base = usize::MAX / 8 - short;
                subject.bit_pos = subject.base * 8;
            }
            subject
        };
        runs.extend(in_step(
            make,
            &program,
            Source::read_bits,
            BufSource::compact,
            &format!("case {case}"),
        ));
    }
    assert!(runs.iter().filter(|run| run.is_ok()).count() > 2_000);
    assert!(
        met(&runs, |kind| matches!(
            kind,
            ErrorKind::UnexpectedEof { .. }
        )) > 500
    );
    assert!(met(&runs, |kind| matches!(kind, ErrorKind::Io(_))) > 100);
    assert!(met(&runs, |kind| matches!(kind, ErrorKind::BufferFull { .. })) > 100);
    assert!(met(&runs, |kind| matches!(kind, ErrorKind::PositionOverflow)) > 100);
}

/// A reader that counts what it is asked for.
struct Asked<'a> {
    data: &'a [u8],
    calls: usize,
    largest: usize,
}

impl io::Read for Asked<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        self.largest = self.largest.max(out.len());
        self.data.read(out)
    }
}

fn asked(data: &[u8]) -> Asked<'_> {
    Asked {
        data,
        calls: 0,
        largest: 0,
    }
}

#[test]
fn a_run_is_asked_of_the_reader_in_as_few_reads_as_it_takes() {
    let data = vec![0x5A; 30_000];

    let mut stream = StreamBitReader::new(asked(&data));
    assert_eq!(stream.read_bytes(100).unwrap(), [0x5A; 100]);
    stream.read_into(&mut [0; 16]).unwrap();
    assert_eq!(stream.inner.calls, 2);
    // So is a value of whole bytes, unless it starts inside a byte.
    assert_eq!(stream.read::<u64>().unwrap(), 0x5A5A_5A5A_5A5A_5A5A);
    assert_eq!(stream.inner.calls, 3);
    stream.read_bits(4).unwrap();
    stream.read::<u32>().unwrap();
    stream.read_bits(4).unwrap();
    assert_eq!(stream.inner.calls, 8);
    stream.inner.calls = 2;
    // A long run is asked for a step at a time, each as large as what has come.
    assert_eq!(
        stream.read_bytes(RUN_STEP + 900).unwrap().len(),
        RUN_STEP + 900
    );
    assert_eq!((stream.inner.calls, stream.inner.largest), (4, RUN_STEP));
    assert_eq!(stream.bit_pos(), (129 + RUN_STEP + 900) * 8);
    stream.read_bytes(4 * RUN_STEP).unwrap();
    assert_eq!(
        (stream.inner.calls, stream.inner.largest),
        (7, 2 * RUN_STEP)
    );

    let mut retaining = BufSource::new(asked(&data));
    assert_eq!(retaining.read_bytes(100).unwrap(), [0x5A; 100]);
    retaining.read_into(&mut [0; 16]).unwrap();
    assert_eq!(retaining.inner.calls, 2);
    assert_eq!(
        retaining.read_bytes(RUN_STEP + 900).unwrap().len(),
        RUN_STEP + 900
    );
    assert_eq!(
        (retaining.inner.calls, retaining.inner.largest),
        (4, RUN_STEP)
    );
    assert_eq!(retaining.bit_pos(), (116 + RUN_STEP + 900) * 8);
    // What is retained counts as what has come: released, the steps start over.
    retaining.compact();
    retaining.read_bytes(4 * RUN_STEP).unwrap();
    assert_eq!(
        (retaining.inner.calls, retaining.inner.largest),
        (7, 2 * RUN_STEP)
    );
}

/// A reader that hands over one byte a call, leaves its mark on everything it was
/// lent, and counts the bytes it finds cleared.
struct Trickle {
    left: usize,
    cleared: usize,
}

impl io::Read for Trickle {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 || out.is_empty() {
            return Ok(0);
        }
        self.cleared += out
            .iter()
            .map(|&byte| usize::from(byte == 0))
            .sum::<usize>();
        out.fill(0xA5);
        self.left -= 1;
        Ok(1)
    }
}

#[test]
fn a_reader_that_trickles_is_not_lent_a_freshly_cleared_step_for_every_read() {
    // What is cleared is the work done besides the reader's: a step cleared again
    // for each byte that comes would be the square of the run.
    let run = 3 * RUN_STEP - 300;
    let trickle = || Trickle {
        left: run,
        cleared: 0,
    };

    let mut stream = StreamBitReader::new(trickle());
    assert_eq!(stream.read_bytes(run).unwrap(), vec![0xA5; run]);
    assert!(stream.inner.cleared <= 2 * run, "{}", stream.inner.cleared);

    let mut retaining = BufSource::new(trickle());
    assert_eq!(retaining.read_bytes(run).unwrap(), vec![0xA5; run]);
    assert!(
        retaining.inner.cleared <= 2 * run,
        "{}",
        retaining.inner.cleared
    );
}

/// A reader that says it filled more than it was lent.
struct Boastful;

impl io::Read for Boastful {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        out.fill(7);
        // One honest byte where there is room for more, so the claim that
        // follows meets a buffer already part filled.
        Ok(if out.len() > 2 { 1 } else { usize::MAX })
    }
}

#[test]
fn a_reader_that_claims_more_than_it_was_lent_is_held_to_what_it_was_lent() {
    let mut stream = StreamBitReader::new(Boastful);
    assert_eq!(stream.read::<u32>().unwrap(), 0x0707_0707);
    assert_eq!(stream.read_bytes(5).unwrap(), [7; 5]);
    let mut three = [0; 3];
    stream.read_into(&mut three).unwrap();
    assert_eq!((three, stream.bit_pos()), ([7; 3], 96));

    let mut retaining = BufSource::new(Boastful);
    assert_eq!(retaining.read::<u32>().unwrap(), 0x0707_0707);
    assert_eq!(retaining.read_bytes(5).unwrap(), [7; 5]);
    retaining.read_into(&mut three).unwrap();
    assert_eq!((three, retaining.bit_pos()), ([7; 3], 96));
}

#[test]
fn a_count_no_input_backs_does_not_size_what_is_asked_or_held() {
    let data = [7; 10];

    let mut stream = StreamBitReader::new(asked(&data));
    let error = stream.read_bytes(usize::MAX / 16).unwrap_err();
    assert_eq!(
        (error.kind, error.at),
        (ErrorKind::Incomplete { needed: None }, 80)
    );
    assert_eq!(stream.inner.largest, RUN_STEP);

    let mut retaining = BufSource::with_capacity(asked(&data), usize::MAX);
    let error = retaining.read_bytes(usize::MAX / 16).unwrap_err();
    assert_eq!(
        (error.kind, error.at),
        (
            ErrorKind::UnexpectedEof {
                needed: 8,
                remaining: 0
            },
            80
        )
    );
    assert_eq!(retaining.inner.largest, RUN_STEP);
    assert!(retaining.buf.capacity() <= 2 * RUN_STEP);
}
