//! Converter selection and execution: `xdsp/conv.c::get_transform_fn` and the generic
//! (scalar) kernels it dispatches to.

use core::cmp;
use core::error::Error;
use core::fmt;

use super::format::{Element, SampleFormat, pack_lone, unpack_lone};

/// Most values one group carries: a complex sample for each of eight channels.
const GROUP_VALUES: usize = 16;

/// A sample conversion between formats, optionally splitting one stream into channels or
/// merging channels into one stream: libusdr's `transform_info_t` for one
/// `get_transform_fn(from, to, inputs, outputs)` call.
///
/// Splitting deals one sample (one complex pair, or one value for `I16` → `F32`) to each
/// output in turn; merging takes one from each input in turn. Byte counts are totals over
/// all channels, as in libusdr, and every channel's buffer on one side has the same length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Converter {
    /// Input channel count.
    inputs: usize,
    /// Output channel count.
    outputs: usize,
    /// How the bytes move.
    plan: Plan,
}

/// The kernel a [`Converter`] runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Plan {
    /// libusdr's `tr_dummy`: copy as many bytes as both buffers hold.
    Copy,
    /// Whole groups of samples, each decoded to `i16` and encoded again, then a `tail`.
    Groups {
        /// Input value encoding.
        from: Element,
        /// Output value encoding.
        to: Element,
        /// Values per channel per group.
        values: usize,
        /// What the kernel does with input left after the last whole group.
        tail: Tail,
    },
}

/// libusdr's handling of input shorter than a group, after the last whole one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tail {
    /// Left unconverted ("do nothing with leftover").
    Ignored,
    /// One-to-one 12-bit kernels convert a lone last value (`pack_lone`/`unpack_lone`).
    Lone,
    /// `ci12` to two `cf32`: a last whole pair goes to channel 0, then a lone value to
    /// the other channel, or to channel 0 if no pair preceded it.
    Ci12Split,
}

impl Converter {
    /// Selects the converter libusdr's `get_transform_fn` returns for these formats and
    /// channel counts.
    ///
    /// A format converted to itself one-to-one is a copy. libusdr also returns that copy for
    /// every other one-to-one pair it does not know (`ci16` to `f32`, say); those are
    /// rejected here, as are all shapes for which it returns no converter.
    ///
    /// # Errors
    ///
    /// [`ConversionError::Unsupported`] for a pair and shape libusdr has no kernel for.
    pub fn new(
        from: SampleFormat,
        to: SampleFormat,
        inputs: usize,
        outputs: usize,
    ) -> Result<Self, ConversionError> {
        use SampleFormat::{Cf32, Ci12, Ci16, F32, I12, I16};

        let groups = |values, tail| Plan::Groups {
            from: from.element(),
            to: to.element(),
            values,
            tail,
        };
        let plan = match (from, to, inputs, outputs) {
            _ if from == to && inputs == 1 && outputs == 1 => Plan::Copy,
            (I16, F32, 1, 1 | 4) | (F32, I16, 1, 1) | (Ci16, Cf32, 1, 1) | (Cf32, Ci16, 1, 1) => {
                groups(1, Tail::Ignored)
            }
            (I12, F32 | I16, 1, 1)
            | (F32 | I16, I12, 1, 1)
            | (Ci12, Cf32 | Ci16, 1, 1)
            | (Cf32 | Ci16, Ci12, 1, 1) => groups(2, Tail::Lone),
            (Ci12, Cf32, 1, 2) => groups(2, Tail::Ci12Split),
            (Ci16, Cf32 | Ci16, 1, 2 | 3 | 4 | 6 | 8)
            | (Ci12, Cf32, 1, 4)
            | (Ci12, Ci16, 1, 2 | 4)
            | (Cf32 | Ci16, Ci16, 2..=4, 1)
            | (Cf32 | Ci16, Ci12, 2 | 4, 1) => groups(2, Tail::Ignored),
            _ => {
                return Err(ConversionError::Unsupported {
                    from,
                    to,
                    inputs,
                    outputs,
                });
            }
        };
        Ok(Self {
            inputs,
            outputs,
            plan,
        })
    }

    /// Total output bytes for `input_bytes` total input bytes: libusdr's size function
    /// (`sfunc(bytes, false)`). It scales by the value sizes and ignores grouping, so it can
    /// exceed what [`convert`](Self::convert) writes for a partial group.
    ///
    /// libusdr computes this in 32-bit `unsigned` arithmetic, which wraps for large counts;
    /// this is exact, saturating at `usize::MAX`.
    #[must_use]
    pub fn output_bytes(&self, input_bytes: usize) -> usize {
        match self.plan {
            Plan::Copy => input_bytes,
            Plan::Groups { from, to, .. } => scale(input_bytes, to.bits(), from.bits()),
        }
    }

    /// Total input bytes that make `output_bytes` total output bytes: libusdr's
    /// `sfunc(bytes, true)`, with [`output_bytes`](Self::output_bytes)' arithmetic.
    #[must_use]
    pub fn input_bytes(&self, output_bytes: usize) -> usize {
        match self.plan {
            Plan::Copy => output_bytes,
            Plan::Groups { from, to, .. } => scale(output_bytes, from.bits(), to.bits()),
        }
    }

    /// Converts `inputs` into `outputs`, one buffer per channel, as libusdr's generic kernel
    /// does when passed their total lengths.
    ///
    /// Only input that fits the outputs is read (libusdr's byte budget). Output past what
    /// that input fills is left untouched, and so is any partial group libusdr's kernel
    /// ignores. Where libusdr would write past a channel buffer (the `ci12` to two `cf32`
    /// tail with an output too short for it), the bytes that fit are written and the rest
    /// dropped.
    ///
    /// # Errors
    ///
    /// [`ConversionError::ChannelCount`] when the buffer counts differ from the converter's,
    /// and [`ConversionError::UnequalLengths`] when one side's buffers differ in length.
    pub fn convert(
        &self,
        inputs: &[&[u8]],
        outputs: &mut [&mut [u8]],
    ) -> Result<(), ConversionError> {
        if inputs.len() != self.inputs || outputs.len() != self.outputs {
            return Err(ConversionError::ChannelCount {
                expected: (self.inputs, self.outputs),
                actual: (inputs.len(), outputs.len()),
            });
        }
        let input_len = common_len(inputs.iter().map(|input| input.len()))?;
        let output_len = common_len(outputs.iter().map(|output| output.len()))?;
        match self.plan {
            Plan::Copy => copy(inputs, outputs),
            Plan::Groups {
                from,
                to,
                values,
                tail,
            } => {
                let budget = cmp::min(
                    input_len.saturating_mul(inputs.len()),
                    scale(
                        output_len.saturating_mul(outputs.len()),
                        from.bits(),
                        to.bits(),
                    ),
                );
                let kernel = Kernel {
                    from,
                    to,
                    values: values * cmp::max(inputs.len(), outputs.len()),
                };
                let rest = kernel.groups(budget, inputs, outputs);
                kernel.tail(tail, rest, inputs, outputs);
            }
        }
        Ok(())
    }
}

/// One run of a [`Plan::Groups`] kernel.
struct Kernel {
    /// Input value encoding.
    from: Element,
    /// Output value encoding.
    to: Element,
    /// Values per group over all channels.
    values: usize,
}

impl Kernel {
    /// Converts every whole group within `budget` total input bytes; returns where the
    /// input stops and how many budgeted bytes are left.
    fn groups(&self, budget: usize, inputs: &[&[u8]], outputs: &mut [&mut [u8]]) -> Rest {
        let group_bytes = self.values * self.from.bits() / 8;
        let input_values = self.values / inputs.len();
        let input_step = input_values * self.from.bits() / 8;
        let output_values = self.values / outputs.len();
        let output_step = output_values * self.to.bits() / 8;
        let groups = budget / group_bytes;

        let mut scratch = [0; GROUP_VALUES];
        // `Converter::new` admits at most two values for each of eight channels.
        let group = &mut scratch[..self.values];
        for index in 0..groups {
            for (input, values) in inputs.iter().zip(group.chunks_exact_mut(input_values)) {
                let bytes = input.get(index * input_step..).unwrap_or_default();
                self.from.decode(bytes, values);
            }
            for (output, values) in outputs.iter_mut().zip(group.chunks_exact(output_values)) {
                let bytes = output.get_mut(index * output_step..).unwrap_or_default();
                self.to.encode(values, bytes);
            }
        }
        Rest {
            input_at: groups * input_step,
            output_at: groups * output_step,
            bytes: budget - groups * group_bytes,
        }
    }

    /// Runs `tail` on the input the whole groups left. Both tails exist only for
    /// single-input kernels.
    fn tail(&self, tail: Tail, rest: Rest, inputs: &[&[u8]], outputs: &mut [&mut [u8]]) {
        let [input] = inputs else { return };
        let input = input.get(rest.input_at..).unwrap_or_default();
        match (tail, outputs) {
            (Tail::Lone, [output]) => {
                let output = output.get_mut(rest.output_at..).unwrap_or_default();
                if self.from == Element::I12 {
                    self.lone_twelve_bit(input, rest.bytes, output);
                } else if rest.bytes >= self.from.bits() / 8 {
                    let mut value = [0];
                    self.from.decode(input, &mut value);
                    for (byte, packed) in output.iter_mut().zip(pack_lone(value[0])) {
                        *byte = packed;
                    }
                }
            }
            (Tail::Ci12Split, [first, second]) => {
                let first = first.get_mut(rest.output_at..).unwrap_or_default();
                let second = second.get_mut(rest.output_at..).unwrap_or_default();
                let (input, bytes, lone_output) = if rest.bytes >= 3 {
                    let mut pair = [0; 2];
                    self.from.decode(input, &mut pair);
                    self.to.encode(&pair, first);
                    (input.get(3..).unwrap_or_default(), rest.bytes - 3, second)
                } else {
                    (input, rest.bytes, first)
                };
                self.lone_twelve_bit(input, bytes, lone_output);
            }
            // `Tail::Ignored`; `Converter::new` pairs the other tails with their shapes.
            _ => {}
        }
    }

    /// Converts a lone 12-bit value when at least two input bytes remain.
    fn lone_twelve_bit(&self, input: &[u8], bytes: usize, output: &mut [u8]) {
        if bytes < 2 {
            return;
        }
        let Some(&[low, high]) = input.get(..2) else {
            return;
        };
        self.to.encode(&[unpack_lone([low, high])], output);
    }
}

/// Where the whole groups stopped.
#[derive(Clone, Copy)]
struct Rest {
    /// Offset into the input buffer.
    input_at: usize,
    /// Offset into each output buffer.
    output_at: usize,
    /// Budgeted input bytes not yet converted.
    bytes: usize,
}

/// libusdr's `tr_dummy`: copies as many bytes as both buffers hold.
fn copy(inputs: &[&[u8]], outputs: &mut [&mut [u8]]) {
    let ([input], [output]) = (inputs, outputs) else {
        return;
    };
    let len = cmp::min(input.len(), output.len());
    output[..len].copy_from_slice(&input[..len]);
}

/// The length every buffer shares, or 0 for none.
fn common_len(mut lens: impl Iterator<Item = usize>) -> Result<usize, ConversionError> {
    let first = lens.next().unwrap_or(0);
    if lens.all(|len| len == first) {
        Ok(first)
    } else {
        Err(ConversionError::UnequalLengths)
    }
}

/// `floor(bytes * numerator / denominator)`, exact and saturating rather than overflowing.
fn scale(bytes: usize, numerator: usize, denominator: usize) -> usize {
    (bytes / denominator)
        .saturating_mul(numerator)
        .saturating_add(bytes % denominator * numerator / denominator)
}

/// Why a conversion cannot be selected or run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConversionError {
    /// libusdr has no converter for this format pair and channel shape.
    Unsupported {
        /// Input format.
        from: SampleFormat,
        /// Output format.
        to: SampleFormat,
        /// Input channel count.
        inputs: usize,
        /// Output channel count.
        outputs: usize,
    },
    /// The buffers do not match the converter's channel counts.
    ChannelCount {
        /// Input and output channels the converter takes.
        expected: (usize, usize),
        /// Input and output buffers passed.
        actual: (usize, usize),
    },
    /// The buffers on one side differ in length.
    UnequalLengths,
}

impl fmt::Display for ConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported {
                from,
                to,
                inputs,
                outputs,
            } => write!(
                f,
                "no {from:?} to {to:?} conversion from {inputs} to {outputs} channels"
            ),
            Self::ChannelCount { expected, actual } => write!(
                f,
                "converter takes {} inputs and {} outputs, given {} and {}",
                expected.0, expected.1, actual.0, actual.1
            ),
            Self::UnequalLengths => f.write_str("channel buffers differ in length"),
        }
    }
}

impl Error for ConversionError {}
