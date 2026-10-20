//! Recorded sessions: the [`Trace`] file format and [`Replay`], which plays one back
//! against a driver.
//!
//! ```text
//! # SYNTHETIC example: the layout only, no real dongle's transfers
//! rtlsdr-trace 1
//! capture: SYNTHETIC-EXAMPLE
//! sha256: -
//! synthetic: yes
//! ---
//! out 40 00 0000 0000 01
//! in c0 00 0000 0000 0001 96
//! ```
//!
//! The first line names the format. Header lines are `key: value` up to `---`: `capture`
//! (the ledger's capture id) and `synthetic` (`yes` for a hand-written trace, `no` for one
//! taken from a capture) are required, and `sha256` (the capture file's, or `-`) is
//! required to be a hash when `synthetic: no`. Then one control transfer per line in
//! [`Transfer`]'s format, in order. Blank lines and lines starting with `#` are ignored
//! anywhere.

use std::fmt;

use crate::trace::{Control, Direction, Transfer};

/// The format's first line.
const MAGIC: &str = "rtlsdr-trace 1";

/// A trace file that does not parse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// The 1-based line.
    pub line: usize,

    /// What is wrong with it.
    pub reason: &'static str,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "trace line {}: {}", self.line, self.reason)
    }
}

impl std::error::Error for ParseError {}

/// A recorded session; only [`Trace::parse`] makes one, so every IN transfer's data fits
/// its `wLength`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trace {
    /// The capture id the ledger cites.
    capture: String,

    /// The capture file's SHA-256; always present for a non-synthetic trace.
    sha256: Option<String>,

    /// Whether the session was written by hand rather than captured.
    synthetic: bool,

    /// The transfers, each with its 1-based line.
    transfers: Vec<(usize, Transfer)>,
}

impl Trace {
    /// The capture id the ledger cites.
    #[must_use]
    pub fn capture(&self) -> &str {
        &self.capture
    }

    /// The capture file's SHA-256, if any.
    #[must_use]
    pub fn sha256(&self) -> Option<&str> {
        self.sha256.as_deref()
    }

    /// Whether the session was written by hand rather than captured.
    #[must_use]
    pub fn is_synthetic(&self) -> bool {
        self.synthetic
    }

    /// The transfers, each with its 1-based line.
    #[must_use]
    pub fn transfers(&self) -> &[(usize, Transfer)] {
        &self.transfers
    }

    /// Parses a trace file.
    ///
    /// # Errors
    ///
    /// [`ParseError`] naming the first bad line.
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let mut lines = text
            .lines()
            .enumerate()
            .map(|(number, line)| (number + 1, line.trim()))
            .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'));
        let error = |line, reason| ParseError { line, reason };
        match lines.next() {
            Some((_, MAGIC)) => {}
            Some((line, _)) => return Err(error(line, "not an rtlsdr-trace 1 file")),
            None => return Err(error(1, "empty")),
        }
        let (mut capture, mut sha256, mut synthetic) = (None, None, None);
        let mut end = None;
        for (line, text) in lines.by_ref() {
            if text == "---" {
                end = Some(line);
                break;
            }
            let (key, value) = text
                .split_once(':')
                .ok_or(error(line, "header line is not key: value"))?;
            let value = value.trim();
            match key.trim() {
                "capture" if !value.is_empty() => capture = Some(value.to_owned()),
                "sha256" if value == "-" => {}
                "sha256" if value.len() == 64 && is_hex(value) => {
                    sha256 = Some(value.to_ascii_lowercase());
                }
                "sha256" => return Err(error(line, "sha256 is 64 hex digits or -")),
                "synthetic" => {
                    synthetic = Some(match value {
                        "yes" => true,
                        "no" => false,
                        _ => return Err(error(line, "synthetic is yes or no")),
                    });
                }
                _ => return Err(error(line, "unknown or empty header key")),
            }
        }
        let end = end.ok_or(error(text.lines().count(), "header has no --- line"))?;
        let capture = capture.ok_or(error(end, "header has no capture id"))?;
        let synthetic = synthetic.ok_or(error(end, "header does not say synthetic: yes/no"))?;
        if !synthetic && sha256.is_none() {
            return Err(error(end, "a captured trace needs its capture's sha256"));
        }
        let transfers = lines
            .map(|(line, text)| {
                parse_transfer(text)
                    .map(|t| (line, t))
                    .ok_or(error(line, "bad transfer"))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            capture,
            sha256,
            synthetic,
            transfers,
        })
    }
}

/// Writes the file [`Trace::parse`] reads.
impl fmt::Display for Trace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{MAGIC}")?;
        writeln!(f, "capture: {}", self.capture)?;
        writeln!(f, "sha256: {}", self.sha256.as_deref().unwrap_or("-"))?;
        writeln!(
            f,
            "synthetic: {}",
            if self.synthetic { "yes" } else { "no" }
        )?;
        writeln!(f, "---")?;
        self.transfers
            .iter()
            .try_for_each(|(_, transfer)| writeln!(f, "{transfer}"))
    }
}

/// Whether `text` is all hex digits (`from_str_radix` alone also takes a leading `+`).
fn is_hex(text: &str) -> bool {
    text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// One transfer line.
fn parse_transfer(text: &str) -> Option<Transfer> {
    let fields: Vec<&str> = text.split_whitespace().collect();
    let hex8 = |s: &str| {
        (s.len() == 2 && is_hex(s))
            .then(|| u8::from_str_radix(s, 16).ok())
            .flatten()
    };
    let hex16 = |s: &str| {
        (s.len() == 4 && is_hex(s))
            .then(|| u16::from_str_radix(s, 16).ok())
            .flatten()
    };
    let (direction, rest) = fields.split_first()?;
    let [request_type, request, value, index, ..] = rest else {
        return None;
    };
    let control = Control {
        request_type: hex8(request_type)?,
        request: hex8(request)?,
        value: hex16(value)?,
        index: hex16(index)?,
    };
    match (*direction, &rest[4..]) {
        ("in", [len, data]) => {
            let len = hex16(len)?;
            let data = parse_data(data)?;
            (data.len() <= usize::from(len)).then_some(Transfer::In { control, len, data })
        }
        ("out", [data]) => Some(Transfer::Out {
            control,
            data: parse_data(data)?,
        }),
        _ => None,
    }
}

/// A transfer line's data field.
fn parse_data(text: &str) -> Option<Vec<u8>> {
    if text == "-" {
        return Some(Vec::new());
    }
    if text.len() % 2 != 0 || !is_hex(text) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(text.get(at..at + 2)?, 16).ok())
        .collect()
}

/// Where a driver session left the recorded one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Divergence {
    /// The trace line expected next, or `None` past its end.
    pub line: Option<usize>,

    /// What the trace expected there.
    pub expected: Option<Transfer>,

    /// What the driver did instead, or `None` if it stopped early. An IN transfer's data
    /// is empty: the driver supplies only its length.
    pub actual: Option<Transfer>,
}

impl fmt::Display for Divergence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let show =
            |t: &Option<Transfer>| t.as_ref().map_or("nothing".to_owned(), ToString::to_string);
        match self.line {
            Some(line) => write!(f, "trace line {line}: ")?,
            None => f.write_str("past the trace's end: ")?,
        }
        write!(
            f,
            "expected {}, driver did {}",
            show(&self.expected),
            show(&self.actual)
        )
    }
}

impl std::error::Error for Divergence {}

/// A trace being played back: each driver transfer must be the next recorded one, and IN
/// transfers are answered with the recorded data.
#[derive(Debug)]
pub struct Replay {
    /// The session.
    trace: Trace,

    /// The next transfer's position in it.
    next: usize,
}

impl Replay {
    /// Plays `trace` from its start.
    #[must_use]
    pub fn new(trace: Trace) -> Self {
        Self { trace, next: 0 }
    }

    /// The driver reads into `data`.
    ///
    /// # Errors
    ///
    /// [`Divergence`] unless the next recorded transfer is an IN with this setup and a
    /// `wLength` of `data.len()`.
    pub fn control_in(&mut self, control: Control, data: &mut [u8]) -> Result<usize, Divergence> {
        let actual = Transfer::In {
            control,
            len: u16::try_from(data.len()).unwrap_or(u16::MAX),
            data: Vec::new(),
        };
        let recorded = self.expect(actual, Direction::In)?;
        let Transfer::In { data: sent, .. } = recorded else {
            unreachable!("expect matched the direction");
        };
        // A parsed trace's data fits wLength, which matched data.len(); min keeps it so.
        let len = sent.len().min(data.len());
        data[..len].copy_from_slice(&sent[..len]);
        Ok(len)
    }

    /// The driver writes `data`.
    ///
    /// # Errors
    ///
    /// [`Divergence`] unless the next recorded transfer is this OUT, data included.
    pub fn control_out(&mut self, control: Control, data: &[u8]) -> Result<(), Divergence> {
        let actual = Transfer::Out {
            control,
            data: data.to_vec(),
        };
        self.expect(actual, Direction::Out).map(drop)
    }

    /// Ends the session.
    ///
    /// # Errors
    ///
    /// [`Divergence`] if recorded transfers remain.
    pub fn finish(&self) -> Result<(), Divergence> {
        match self.trace.transfers.get(self.next) {
            None => Ok(()),
            Some((line, expected)) => Err(Divergence {
                line: Some(*line),
                expected: Some(expected.clone()),
                actual: None,
            }),
        }
    }

    /// Consumes the next recorded transfer if `actual` matches it.
    fn expect(&mut self, actual: Transfer, direction: Direction) -> Result<Transfer, Divergence> {
        let Some((line, expected)) = self.trace.transfers.get(self.next) else {
            return Err(Divergence {
                line: None,
                expected: None,
                actual: Some(actual),
            });
        };
        let matches = match (direction, expected, &actual) {
            (
                Direction::In,
                Transfer::In { control, len, .. },
                Transfer::In {
                    control: c, len: l, ..
                },
            ) => control == c && len == l,
            (Direction::Out, expected, actual) => expected == actual,
            _ => false,
        };
        if !matches {
            return Err(Divergence {
                line: Some(*line),
                expected: Some(expected.clone()),
                actual: Some(actual),
            });
        }
        self.next += 1;
        Ok(expected.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal well-formed file, SYNTHETIC: its values exercise the format only.
    const FILE: &str = "\
# SYNTHETIC: format test only
rtlsdr-trace 1
capture: SYNTHETIC-TEST
synthetic: yes
---
out 40 01 0002 0003 0a0b
in c0 01 0002 0003 0002 0c
";

    #[test]
    fn a_trace_round_trips_through_its_text() {
        let trace = Trace::parse(FILE).unwrap();
        assert_eq!(trace.capture, "SYNTHETIC-TEST");
        assert!(trace.synthetic);
        assert_eq!(trace.sha256, None);
        assert_eq!(trace.transfers.len(), 2);
        assert_eq!(
            trace.transfers[0].0, 6,
            "lines are counted from the file's start"
        );
        assert_eq!(
            Trace::parse(&trace.to_string()).unwrap().transfers,
            trace.transfers
        );
    }

    #[test]
    fn malformed_files_name_their_line() {
        let bad = |text: &str| Trace::parse(text).unwrap_err().line;
        let body =
            |line: &str| format!("rtlsdr-trace 1\ncapture: X\nsynthetic: yes\n---\n{line}\n");
        assert_eq!(bad("rtlsdr-trace 2\n"), 1);
        assert_eq!(
            bad("rtlsdr-trace 1\nsynthetic: yes\n---\n"),
            3,
            "no capture id"
        );
        assert_eq!(
            bad("rtlsdr-trace 1\ncapture: X\n---\n"),
            3,
            "synthetic unstated"
        );
        assert_eq!(
            bad("rtlsdr-trace 1\ncapture: X\nsynthetic: no\n---\n"),
            4,
            "a captured trace without its hash"
        );
        assert_eq!(
            bad("rtlsdr-trace 1\ncapture: X\nsha256: 12\n"),
            3,
            "short hash"
        );
        assert_eq!(
            bad(&body("in c0 01 0002 0003 0001 0c0d")),
            5,
            "data longer than wLength"
        );
        assert_eq!(bad(&body("out 40 01 2 3 -")), 5, "fixed-width fields");
        assert_eq!(bad(&body("out 40 01 0002 0003 abc")), 5, "odd data");
        assert_eq!(
            bad(&body("out +4 01 0002 0003 -")),
            5,
            "a sign is not a hex digit"
        );
        assert_eq!(
            bad(&body("out 40 01 +002 0003 -")),
            5,
            "a sign is not a hex digit"
        );
        assert_eq!(
            bad(&body("out 40 01 0002 0003 +a")),
            5,
            "a sign is not a hex digit"
        );
    }

    #[test]
    fn a_captured_trace_keeps_its_hash() {
        let hash = "AB".repeat(32);
        let text = format!("rtlsdr-trace 1\ncapture: CAP-X\nsha256: {hash}\nsynthetic: no\n---\n");
        let trace = Trace::parse(&text).unwrap();
        assert_eq!(trace.sha256(), Some("ab".repeat(32).as_str()));
        assert!(!trace.is_synthetic());
    }

    #[test]
    fn generated_traces_print_and_parse_back_unchanged() {
        // A seeded linear congruential generator: deterministic, no dependency.
        let mut state: u64 = 0x5EED;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            u16::try_from(state >> 48).unwrap()
        };
        for _ in 0..200 {
            let mut transfers = Vec::new();
            for line in 0..usize::from(next() % 8) {
                let [request_type, request] = next().to_be_bytes();
                let control = Control {
                    request_type,
                    request,
                    value: next(),
                    index: next(),
                };
                let data: Vec<u8> = (0..next() % 5).map(|_| next().to_be_bytes()[0]).collect();
                let transfer = if next() % 2 == 0 {
                    let len = u16::try_from(data.len()).unwrap() + next() % 3;
                    Transfer::In { control, len, data }
                } else {
                    Transfer::Out { control, data }
                };
                transfers.push((line + 6, transfer));
            }
            let trace = Trace {
                capture: "SYNTHETIC-GEN".to_owned(),
                sha256: None,
                synthetic: true,
                transfers,
            };
            assert_eq!(Trace::parse(&trace.to_string()), Ok(trace));
        }
    }
}
