//! **tcp** — a TCP (RFC 9293) segment-header codec on the [`bnb`] bit-aware codec.
//!
//! From-scratch and dual-use: [`TcpHeader`] decodes/encodes the 20-byte fixed header plus any
//! options, preserving representable input exactly. `checksum`, the reserved bits, and
//! `data_offset` are stored **verbatim** — decode never recomputes or rejects them, so a
//! forged checksum or a lying data-offset survives a round-trip. Options are kept as raw
//! bytes (a structured TLV parser is a later refinement); a checksum-compute helper (which
//! needs the IP pseudo-header) will arrive with the `rawsock` composition model.
//!
//! This is a **header codec**, not a connection: there is no state machine, retransmission,
//! or I/O here.
//!
//! [`bnb`]: https://github.com/RawSocketLabs/bitsandbytes
//!
//! ```
//! use tcp::{Control, TcpHeader};
//!
//! let syn = TcpHeader::segment(40000, 80, 0x1000, 0, Control::new().with_syn(true), 65535, vec![]);
//! let wire = syn.to_bytes().unwrap();
//! assert_eq!(wire.len(), 20); // no options → a 20-byte header
//! let back = TcpHeader::decode_exact(&wire).unwrap();
//! assert!(back.is_syn() && !back.is_ack());
//! assert_eq!(back.header_len(), 20);
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use bnb::{bin, bitfield, u4};

/// The data-offset + reserved + control-bits word — the 16 bits at RFC 9293 §3.1 byte 12–13.
///
/// A flat `#[bitfield(u16)]`: `data_offset` (header length in 32-bit words) and the reserved
/// nibble, then the eight control bits MSB-first (`CWR ECE URG ACK PSH RST SYN FIN`).
//~ models rfc9293#3.1 part="Data Offset + control bits"
#[bitfield(u16, bits = msb, bytes = be)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Control {
    /// Data offset — the header length in 32-bit words (5 = a 20-byte header, no options).
    pub data_offset: u4,
    /// Reserved (must be zero; preserved verbatim — dual-use).
    pub reserved: u4,
    /// CWR — Congestion Window Reduced (RFC 3168).
    pub cwr: bool,
    /// ECE — ECN-Echo (RFC 3168).
    pub ece: bool,
    /// URG — the urgent-pointer field is significant.
    pub urg: bool,
    /// ACK — the acknowledgment field is significant.
    pub ack: bool,
    /// PSH — push buffered data to the receiving application.
    pub psh: bool,
    /// RST — reset the connection.
    pub rst: bool,
    /// SYN — synchronize sequence numbers.
    pub syn: bool,
    /// FIN — no more data from the sender.
    pub fin: bool,
}

/// A TCP segment header (RFC 9293 §3.1): the 20-byte fixed header plus any options.
//~ models rfc9293#3.1 part="TCP header format"
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TcpHeader {
    /// Source port.
    pub src_port: u16,
    /// Destination port.
    pub dst_port: u16,
    /// Sequence number.
    pub seq: u32,
    /// Acknowledgment number (significant when `ack` is set).
    pub ack: u32,
    /// Data offset, reserved bits, and the control (flag) bits.
    pub control: Control,
    /// The receive-window size.
    pub window: u16,
    /// The checksum (stored verbatim — not recomputed or verified on decode).
    pub checksum: u16,
    /// The urgent pointer (significant when `urg` is set).
    pub urgent: u16,
    /// The options, as raw bytes: `(data_offset - 5) * 4` bytes. `saturating_sub` keeps a
    /// malformed `data_offset < 5` from underflow-panicking on untrusted input (it reads zero
    /// option bytes). A structured TLV view is a later refinement.
    #[br(count = usize::from(u8::from(control.data_offset()).saturating_sub(5)) * 4)]
    pub options: Vec<u8>,
}

impl TcpHeader {
    /// The maximum data-offset value (15 words = a 60-byte header, 40 bytes of options).
    const MAX_DATA_OFFSET: usize = 15;

    /// Build a segment, computing `data_offset` from the options length (options are padded
    /// to a 4-byte boundary per the wire format). `flags` supplies the control bits; its own
    /// `data_offset` is overwritten. `checksum` and `urgent` default to zero — set them (or
    /// forge them) afterward. To emit a header whose `data_offset` deliberately *disagrees*
    /// with its options (dual-use), construct the struct directly.
    #[must_use]
    pub fn segment(
        src_port: u16,
        dst_port: u16,
        seq: u32,
        ack: u32,
        flags: Control,
        window: u16,
        options: Vec<u8>,
    ) -> Self {
        let words = (5 + options.len().div_ceil(4)).min(Self::MAX_DATA_OFFSET);
        let control = flags.with_data_offset(u4::new(words as u8));
        Self {
            src_port,
            dst_port,
            seq,
            ack,
            control,
            window,
            checksum: 0,
            urgent: 0,
            options,
        }
    }

    /// The header length in bytes (`data_offset * 4`).
    #[must_use]
    pub fn header_len(&self) -> usize {
        usize::from(u8::from(self.control.data_offset())) * 4
    }

    /// Whether the SYN flag is set.
    #[must_use]
    pub fn is_syn(&self) -> bool {
        self.control.syn()
    }

    /// Whether the ACK flag is set.
    #[must_use]
    pub fn is_ack(&self) -> bool {
        self.control.ack()
    }

    /// Whether the FIN flag is set.
    #[must_use]
    pub fn is_fin(&self) -> bool {
        self.control.fin()
    }

    /// Whether the RST flag is set.
    #[must_use]
    pub fn is_rst(&self) -> bool {
        self.control.rst()
    }
}

/// Pure `Control` bitfield / `segment` logic — no wire codec.
#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn control_packs_offset_and_flags_msb_first() {
        let c = Control::new()
            .with_data_offset(u4::new(5))
            .with_syn(true)
            .with_ack(true);
        // data_offset=5 (top nibble → 0x50), reserved=0, low byte: ack(bit4)+syn(bit1)=0x12.
        assert_eq!(c.to_be_bytes(), [0x50, 0x12]);
        let back = Control::from_be_bytes([0x50, 0x12]);
        assert_eq!(u8::from(back.data_offset()), 5);
        assert!(back.syn() && back.ack() && !back.fin() && !back.rst());
    }

    #[test]
    fn segment_computes_data_offset_from_options() {
        let none = TcpHeader::segment(1, 2, 0, 0, Control::new().with_syn(true), 100, vec![]);
        assert_eq!(none.header_len(), 20); // offset 5
        assert!(none.is_syn());

        let one_word = TcpHeader::segment(1, 2, 0, 0, Control::new(), 100, vec![1, 2, 3, 4]);
        assert_eq!(one_word.header_len(), 24); // offset 6

        // 5 option bytes pad up to two 4-byte words → offset 7 (28 bytes).
        let padded = TcpHeader::segment(1, 2, 0, 0, Control::new(), 100, vec![1, 2, 3, 4, 5]);
        assert_eq!(padded.header_len(), 28);
    }
}
