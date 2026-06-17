//! A five-minute tour of every macro.
//!
//! Each block below is a complete, runnable example. Together they cover the whole
//! surface; the later guide pages go deep on each.
//!
//! # 1. Arbitrary-width integers
//!
//! `u1`..`u127` are range-checked sub-byte integers — the building blocks of packed
//! fields. The native widths (`u8`/`u16`/…) are the standard library's.
//!
//! ```
//! use bnb::{u4, u12};
//!
//! let nibble = u4::new(0xA);          // panics if > 0xF
//! assert_eq!(nibble.value(), 0xA);
//! assert!(u4::try_new(0x10).is_err()); // checked construction
//! assert_eq!(u12::MAX.value(), 0xFFF);
//! ```
//!
//! # 2. `#[bitfield]` — pack typed fields into one integer
//!
//! ```
//! use bnb::{bitfield, u4};
//!
//! // A u16 split into three fields, most-significant-first (RFC order).
//! #[bitfield(u16, bits = msb, bytes = be)]
//! #[derive(Clone, Copy)]
//! struct VlanTag {
//!     pcp: u4,   // high nibble
//!     dei: bool,
//!     vid: bnb::u11,
//! }
//!
//! let tag = VlanTag::new().with_pcp(u4::new(5)).with_dei(true).with_vid(bnb::u11::new(100));
//! assert_eq!(tag.pcp().value(), 5);
//! assert!(tag.dei());
//! assert_eq!(tag.vid().value(), 100);
//! assert_eq!(tag.to_be_bytes().len(), 2);
//! ```
//!
//! # 3. `#[derive(BitEnum)]` — enum ⇄ integer
//!
//! A `#[catch_all]` variant preserves unknown values, so a parser never rejects
//! representable input (the dual-use convention).
//!
//! ```
//! use bnb::{BitEnum, Bits, u4};
//!
//! #[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
//! #[bit_enum(u4)]
//! enum RCode {
//!     NoError,
//!     FormErr,
//!     ServFail,
//!     #[catch_all]
//!     Other(u4),
//! }
//!
//! assert_eq!(RCode::from_bits(2), RCode::ServFail);
//! assert_eq!(RCode::from_bits(9), RCode::Other(u4::new(9))); // unknown, preserved
//! assert_eq!(RCode::Other(u4::new(9)).into_bits(), 9);       // round-trips
//! ```
//!
//! # 4. `#[bitflags]` — single-bit flag sets
//!
//! ```
//! use bnb::bitflags;
//!
//! #[bitflags(u8)]
//! #[derive(Clone, Copy)]
//! struct TcpFlags { fin: bool, syn: bool, rst: bool, psh: bool, ack: bool, urg: bool }
//!
//! let f = TcpFlags::SYN | TcpFlags::ACK;
//! assert!(f.contains(TcpFlags::SYN));
//! assert!(f.ack());            // per-flag accessor
//! assert_eq!(f.bits(), 0b0001_0010);
//! ```
//!
//! # 5. `#[bin]` — a whole message
//!
//! `#[bin]` folds the read/write codec and a required-by-default builder over a
//! struct. Fields can be any `Bits` type (including the `#[bitfield]`/`BitEnum`/
//! `#[bitflags]` from above), read and written at arbitrary bit offsets.
//!
//! ```
//! use bnb::bin;
//!
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct UdpHeader {
//!     src_port: u16,
//!     dst_port: u16,
//!     length: u16,
//!     checksum: u16,
//! }
//!
//! let h = UdpHeader::builder()
//!     .src_port(1234).dst_port(53).length(8).checksum(0)
//!     .build()
//!     .unwrap();                       // Err names any field you forgot
//! let bytes = h.to_bytes().unwrap();   // -> [0x04,0xD2, 0x00,0x35, 0x00,0x08, 0x00,0x00]
//! assert_eq!(bytes, [0x04, 0xD2, 0x00, 0x35, 0x00, 0x08, 0x00, 0x00]);
//! assert_eq!(UdpHeader::decode_exact(&bytes).unwrap(), h); // exact inverse
//! ```
//!
//! Next: [`numbers`](super::numbers) for the foundation, or
//! [`bin_codec`](super::bin_codec) for the codec in depth.
