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
    /// The transport set a stream up (`stream_initialize`).
    StreamOpen {
        /// Whether it is the TX stream (stream 1); else RX (stream 0).
        tx: bool,
        /// Bytes per transport block.
        block_bytes: u32,
    },
    /// The board delivered an RX block.
    RxBlock {
        /// Its bytes.
        len: usize,
        /// Its first burst's time.
        time: u64,
    },
    /// The board took a TX block.
    TxBlock {
        /// The block, as committed.
        bytes: Vec<u8>,
        /// The commit's out-of-band time, `None` for as soon as possible.
        time: Option<u64>,
    },
}

/// One line per operation, the line format of the characterization golden traces:
/// `req FRAME`, `rep FRAME` (see [`Frame`]'s `Display`), `fpga-wr ADDR VALUE`,
/// `fpga-rd ADDR VALUE`, `lms7-wr ADDR VALUE`, `lms7-rd ADDR VALUE`, `lms7-reset`,
/// `sleep US`, `stream-open rx|tx BYTES`, `rx-block BYTES TIME` and
/// `tx-block BYTES TIME|- FNV`, the last the block's 64-bit FNV-1a hash. Addresses and
/// values are fixed-width hex.
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
            Self::StreamOpen { tx, block_bytes } => {
                let stream = if *tx { "tx" } else { "rx" };
                write!(f, "stream-open {stream} {block_bytes}")
            }
            Self::RxBlock { len, time } => write!(f, "rx-block {len} {time}"),
            Self::TxBlock { bytes, time } => {
                write!(f, "tx-block {} ", bytes.len())?;
                match time {
                    Some(time) => write!(f, "{time}")?,
                    None => f.write_str("-")?,
                }
                let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, &byte| {
                    (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
                });
                write!(f, " {hash:016x}")
            }
        }
    }
}
