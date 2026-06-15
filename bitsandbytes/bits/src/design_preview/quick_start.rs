//! # Quick start — three complete examples
//!
//! The "feel" of `bnb` across the spectrum, byte-aligned to bit-level. (Target
//! design; ` ```rust,ignore `.)
//!
//! ## 1. A byte-aligned header (the binrw-familiar case)
//!
//! Whole-byte fields, a magic number, a count-driven list. Reads like binrw —
//! because the surface *is* binrw's.
//!
//! ```rust,ignore
//! use bnb::bin;
//!
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct DnsMessage {
//!     id: u16,
//!     flags: DnsFlags,                 // a #[bitfield] — packs into 2 bytes
//!     #[bw(calc = self.questions.len() as u16)]
//!     qdcount: u16,                    // derived on write, temp on read
//!     #[br(count = qdcount)]
//!     questions: Vec<Question>,        // count-driven, pure binrw-style
//! }
//! ```
//!
//! ## 2. A bit-level frame (the case binrw can't express cleanly)
//!
//! Fields at non-byte offsets — the DMR burst from `DESIGN.md`. No `seek_before`,
//! no `from_be_bytes`/`>> 4` shuffling: declare the widths and you're done.
//!
//! ```rust,ignore
//! use bnb::bin;
//!
//! #[bin(big)]                          // bit_order defaults to msb
//! #[derive(Debug, PartialEq)]
//! struct DmrBurst {
//!     payload_1: u108,                  // bits   0..108
//!     sync: SyncPattern,                // bits 108..156  (a 48-bit BitEnum)
//!     payload_2: u108,                  // bits 156..264
//! }
//! // 264 bits = 33 bytes, parsed/written with a bit cursor — no Seek, no NoSeek.
//! ```
//!
//! ## 3. A mixed message (dispatch — both backends, one struct)
//!
//! Byte-aligned framing around a sub-byte region. binrw handles the magic and the
//! trailer; the bit cursor handles the region — same `#[br]`/`#[bw]`/`#[brw]`
//! vocabulary throughout.
//!
//! ```rust,ignore
//! use bnb::bin;
//!
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct Frame {
//!     #[brw(magic = 0x7Eu8)]            // a 1-byte start delimiter
//!     version: u8,
//!
//!     burst: DmrBurst,                  // the sub-byte region (example 2), inline
//!
//!     #[bw(calc = crc16(&..))]          // computed on write
//!     crc: u16,
//! }
//! ```
//!
//! ## Reading and writing
//!
//! The common case takes bytes (or any `Read`) directly — no reader to construct:
//!
//! ```rust,ignore
//! use bnb::{Decode, Encode};
//!
//! // Consume one message from a buffer; `buf` advances past it (tail-tolerant).
//! let mut buf: &[u8] = &bytes;
//! let frame = Frame::decode(&mut buf)?;        // Err(Incomplete) ⇒ need more
//!
//! // Strict variant: require the whole slice to be one message.
//! let frame = Frame::decode_exact(&bytes)?;
//!
//! // Build with the generated builder, then write to anything `Write`:
//! let frame = Frame::builder().version(0x7E).burst(burst).build()?;
//! frame.encode(&mut socket)?;                  // any std::io::Write
//! let bytes = frame.to_bytes()?;               // …or just a Vec<u8>
//! ```
//!
//! For framing a stream, decoding several messages from one buffer, or mid-buffer
//! seeks, drop to the explicit [`BitReader`](super::io_model) — see
//! [`super::io_model`] for the full entry-point set (`decode` / `decode_exact` /
//! `peek` / `decode_from`).
//!
//! Continue to [`super::bin_macro`] for the macro's options, or
//! [`super::attributes`] for the full directive reference.
