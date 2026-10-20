//! Control transfers in the trace line format, and the record of a [`SimDongle`]
//! session.
//!
//! [`SimDongle`]: crate::SimDongle

use std::fmt;

/// A control transfer's setup, as on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Control {
    /// `bmRequestType`.
    pub request_type: u8,

    /// `bRequest`.
    pub request: u8,

    /// `wValue`.
    pub value: u16,

    /// `wIndex`.
    pub index: u16,
}

impl fmt::Display for Control {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02x} {:02x} {:04x} {:04x}",
            self.request_type, self.request, self.value, self.index
        )
    }
}

/// A data stage's direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Device to host.
    In,

    /// Host to device.
    Out,
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::In => "in",
            Self::Out => "out",
        })
    }
}

/// One control transfer, as the device answered it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Transfer {
    /// A read: the setup, `wLength`, and the bytes the device returned.
    In {
        /// The setup.
        control: Control,

        /// `wLength`.
        len: u16,

        /// What the device sent, at most `len` bytes.
        data: Vec<u8>,
    },

    /// A write: the setup and the bytes sent.
    Out {
        /// The setup.
        control: Control,

        /// What the host sent.
        data: Vec<u8>,
    },

    /// A transfer the device refused (stalled); its data stage is not recorded.
    Stall {
        /// Its direction.
        direction: Direction,

        /// Its setup.
        control: Control,
    },
}

/// One line per transfer: `in TT RR VVVV IIII LLLL DATA`, `out TT RR VVVV IIII DATA` or
/// `stall in|out TT RR VVVV IIII`, in fixed-width lowercase hex, `DATA` the bytes run
/// together or `-` when empty.
impl fmt::Display for Transfer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::In { control, len, data } => write!(f, "in {control} {len:04x} {}", Hex(data)),
            Self::Out { control, data } => write!(f, "out {control} {}", Hex(data)),
            Self::Stall { direction, control } => write!(f, "stall {direction} {control}"),
        }
    }
}

/// Bytes as the trace line format writes them.
pub(crate) struct Hex<'a>(pub(crate) &'a [u8]);

impl fmt::Display for Hex<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            return f.write_str("-");
        }
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

/// One operation, as the dongle saw it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Op {
    /// A control transfer, completed or stalled.
    Control(Transfer),

    /// The page write a completed control transfer carried to the tuner.
    TunerWrite {
        /// The first register.
        register: u8,

        /// The bytes written from it on.
        data: Vec<u8>,
    },

    /// The tuner read a completed control transfer carried.
    TunerRead {
        /// How many bytes, from register 0.
        len: usize,
    },

    /// The RX stream started.
    RxStart {
        /// Bytes per transfer.
        transfer_len: usize,

        /// Transfers in flight.
        depth: usize,
    },

    /// One RX transfer was handed over.
    RxRead {
        /// Its length.
        len: usize,
    },

    /// The RX stream stopped.
    RxStop,

    /// The driver slept.
    Sleep {
        /// Virtual microseconds.
        us: u64,
    },
}

/// One line per operation: a [`Transfer`]'s own line, `tuner-wr RR DATA`, `tuner-rd LEN`, `rx-start LEN DEPTH`, `rx-read LEN`, `rx-stop`
/// and `sleep US`.
impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Control(transfer) => write!(f, "{transfer}"),
            Self::TunerWrite { register, data } => {
                write!(f, "tuner-wr {register:02x} {}", Hex(data))
            }
            Self::TunerRead { len } => write!(f, "tuner-rd {len}"),
            Self::RxStart {
                transfer_len,
                depth,
            } => write!(f, "rx-start {transfer_len} {depth}"),
            Self::RxRead { len } => write!(f, "rx-read {len}"),
            Self::RxStop => f.write_str("rx-stop"),
            Self::Sleep { us } => write!(f, "sleep {us}"),
        }
    }
}
