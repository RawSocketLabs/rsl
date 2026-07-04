//! `#[bitflags]` — a named set of single-bit flags with set algebra.
//!
//! Each `bool` field is one flag, assigned a bit by declaration order (LSB-first: the
//! first field is `1 << 0`), or pinned with `#[flag(N)]`.
//!
//! ```
//! use bnb::bitflags;
//!
//! #[bitflags(u8)]
//! #[derive(Clone, Copy, Debug, PartialEq, Eq)]
//! struct TcpFlags {
//!     fin: bool,            // bit 0
//!     syn: bool,            // bit 1
//!     rst: bool,            // bit 2
//!     psh: bool,            // bit 3
//!     ack: bool,            // bit 4
//!     #[flag(7)] cwr: bool, // pinned to bit 7
//! }
//!
//! assert_eq!(TcpFlags::SYN.bits(), 0b0000_0010); // an UPPERCASE const per flag
//! assert_eq!(TcpFlags::CWR.bits(), 0b1000_0000);
//! ```
//!
//! # Set algebra
//!
//! Flags compose with the bitwise operators and the named set operations:
//!
//! ```
//! # use bnb::bitflags;
//! # #[bitflags(u8)] #[derive(Clone, Copy, Debug, PartialEq, Eq)]
//! # struct TcpFlags { fin: bool, syn: bool, rst: bool, psh: bool, ack: bool, #[flag(7)] cwr: bool }
//! let f = TcpFlags::SYN | TcpFlags::ACK;
//! assert!(f.contains(TcpFlags::SYN));
//! assert!(f.intersects(TcpFlags::ACK | TcpFlags::FIN));
//! assert_eq!((f - TcpFlags::SYN), TcpFlags::ACK);   // difference
//! assert_eq!((f & TcpFlags::SYN), TcpFlags::SYN);   // intersection
//! assert!(TcpFlags::empty().is_empty());
//! ```
//!
//! # Per-flag accessors and iteration
//!
//! ```
//! # use bnb::bitflags;
//! # #[bitflags(u8)] #[derive(Clone, Copy, Debug, PartialEq, Eq)]
//! # struct TcpFlags { fin: bool, syn: bool, rst: bool, psh: bool, ack: bool, #[flag(7)] cwr: bool }
//! let mut f = TcpFlags::empty().with_syn(true).with_ack(true);
//! assert!(f.syn() && f.ack() && !f.fin());
//! f.set_ack(false);
//! assert!(!f.ack());
//! let set: Vec<_> = f.iter().collect();            // the single-bit flags that are set
//! assert_eq!(set, vec![TcpFlags::SYN]);
//! ```
//!
//! # Unknown bits: retain vs. truncate
//!
//! Like a catch-all enum, a flag set is dual-use: `from_bits` **retains** bits that
//! don't correspond to a declared flag (so a parser round-trips unknown bits), while
//! `from_bits_truncate` drops them.
//!
//! ```
//! # use bnb::bitflags;
//! # #[bitflags(u8)] #[derive(Clone, Copy, Debug, PartialEq, Eq)]
//! # struct TcpFlags { fin: bool, syn: bool, rst: bool, psh: bool, ack: bool, #[flag(7)] cwr: bool }
//! let raw = 0b0010_0010; // SYN set, plus an undefined bit 5
//! assert_eq!(TcpFlags::from_bits(raw).bits(), 0b0010_0010);          // retained
//! assert_eq!(TcpFlags::from_bits_truncate(raw).bits(), 0b0000_0010); // dropped
//! ```
//!
//! # Nesting in a bitfield
//!
//! A flag set implements [`Bits`](crate::Bits), so it drops into a `#[bitfield]` or a
//! `#[bin]` message as a field of its backing width:
//!
//! ```
//! use bnb::{bitfield, bitflags, u4};
//!
//! #[bitflags(u8)]
//! #[derive(Clone, Copy, Debug, PartialEq, Eq)]
//! struct Caps { dnssec: bool, recursion: bool, compression: bool, extended: bool }
//!
//! #[bitfield(u16, bits = msb)]
//! #[derive(Clone, Copy)]
//! struct Header { version: u4, caps: Caps }   // 4 + 8 = 12 bits (in a u16)
//!
//! let h = Header::new().with_version(u4::new(1)).with_caps(Caps::DNSSEC | Caps::RECURSION);
//! assert!(h.caps().dnssec());
//! ```
