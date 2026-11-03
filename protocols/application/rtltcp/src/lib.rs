//! **rtltcp** — the `rtl_tcp` protocol on the [`bnb`] bit-aware codec.
//!
//! `rtl_tcp` serves an RTL-SDR dongle over TCP. On connect the server sends a 12-byte
//! [`Greeting`]; it then streams interleaved unsigned 8-bit I/Q for as long as the socket is
//! open, while the client sends 5-byte [`Command`]s on the same socket. Everything framed is
//! big-endian.
//!
//! Dual-use: the parser accepts any greeting magic and any opcode (unknown opcodes are
//! preserved as [`Opcode::Other`]); only the optional [`Client`] refuses a server whose
//! greeting is not `RTL0`.
//!
//! ```
//! use rtltcp::{Command, Greeting, Opcode};
//!
//! let greeting = Greeting::decode_exact(b"RTL0\0\0\0\x05\0\0\0\x1d").unwrap();
//! assert!(greeting.has_magic());
//! assert_eq!(greeting.gain_count, 29);
//!
//! let tune = Command::set_center_frequency(462_562_500);
//! assert_eq!(tune.opcode, Opcode::SetCenterFrequency);
//! assert_eq!(tune.to_bytes().unwrap(), [0x01, 0x1b, 0x92, 0x24, 0xc4]);
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod command;
pub mod greeting;

/// A blocking TCP client — the `client` feature.
#[cfg(feature = "client")]
pub mod client;

#[cfg(feature = "client")]
pub use client::{Client, ClientError};
pub use command::{Command, Opcode};
pub use greeting::Greeting;
