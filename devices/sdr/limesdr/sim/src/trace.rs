//! The ordered record of every operation a driver performed on a [`SimBoard`](crate::SimBoard).

use std::fmt;

use crate::Frame;

/// One operation, as seen by the board: each LMS64C exchange is its request frame, the
/// register operations it decoded to, then its reply frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// An LMS64C request frame, as received.
    Request(Frame),
    /// The LMS64C reply frame, as sent.
    Reply(Frame),
    /// A gateware register write.
    FpgaWrite {
        /// Register address.
        addr: u16,
        /// Value written.
        value: u16,
    },
    /// A gateware register read.
    FpgaRead {
        /// Register address.
        addr: u16,
        /// Value returned.
        value: u16,
    },
    /// An LMS7002M register write.
    Lms7Write {
        /// The address field as sent; bit 15 is the SPI write flag, not address.
        addr: u16,
        /// Value written.
        value: u16,
    },
    /// An LMS7002M register read.
    Lms7Read {
        /// The address field as sent.
        addr: u16,
        /// Value returned.
        value: u16,
    },
    /// The LMS7002M's reset pin was pulsed.
    Lms7Reset,
    /// The driver slept.
    Sleep {
        /// Virtual microseconds slept.
        us: u64,
    },
}

/// One line per operation, the line format of the characterization golden traces:
/// `req FRAME`, `rep FRAME` (see [`Frame`]'s `Display`), `fpga-wr ADDR VALUE`,
/// `fpga-rd ADDR VALUE`, `lms7-wr ADDR VALUE`, `lms7-rd ADDR VALUE`, `lms7-reset` and
/// `sleep US`. Addresses and values are fixed-width hex.
impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(frame) => write!(f, "req {frame}"),
            Self::Reply(frame) => write!(f, "rep {frame}"),
            Self::FpgaWrite { addr, value } => write!(f, "fpga-wr {addr:#06x} {value:#06x}"),
            Self::FpgaRead { addr, value } => write!(f, "fpga-rd {addr:#06x} {value:#06x}"),
            Self::Lms7Write { addr, value } => write!(f, "lms7-wr {addr:#06x} {value:#06x}"),
            Self::Lms7Read { addr, value } => write!(f, "lms7-rd {addr:#06x} {value:#06x}"),
            Self::Lms7Reset => f.write_str("lms7-reset"),
            Self::Sleep { us } => write!(f, "sleep {us}"),
        }
    }
}
