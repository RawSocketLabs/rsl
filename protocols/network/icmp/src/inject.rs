//! The `rawsock` injection layer (the `inject` feature) — compose an ICMP header with its data
//! and encode the whole, checksummed message.
//!
//! [`Icmp`] wraps an [`IcmpHeader`](crate::IcmpHeader) plus a payload and implements
//! [`rawsock::Protocol`]. Encoding is **dual-use**:
//! - [`encode`](rawsock::ProtocolExt::encode) (compliant) computes the checksum over the whole
//!   ICMP message. Unlike UDP/TCP this needs **no pseudo-header** — the ICMP checksum is
//!   self-contained — so the enclosing [`Context`] is ignored for the checksum.
//! - [`encode_raw`](rawsock::ProtocolExt::encode_raw) (verbatim) emits `.header.checksum` as
//!   set — forge it by writing the field.

use crate::IcmpHeader;
use rawsock::{Context, Layer, Protocol, internet_checksum};

/// An ICMP layer for rawsock composition: an [`IcmpHeader`](crate::IcmpHeader) over its data
/// payload `P` (raw `Vec<u8>`, or another [`Protocol`] — e.g. an error message's embedded packet).
#[derive(Clone, Debug)]
pub struct Icmp<P> {
    /// The ICMP header. `checksum` is recomputed by the compliant `encode` (over header + data)
    /// and emitted verbatim by `encode_raw` — set it here to forge.
    pub header: IcmpHeader,
    /// The message data (the Echo payload, or an error's embedded packet).
    pub payload: P,
}

impl<P> Icmp<P> {
    /// An ICMP layer wrapping `header` over `payload`. Build `header` with
    /// [`IcmpHeader::echo_request`](crate::IcmpHeader::echo_request) etc.
    pub fn new(header: IcmpHeader, payload: P) -> Self {
        Self { header, payload }
    }
}

/// The 8-byte header (with `checksum`) followed by `payload`.
fn message_bytes(h: &IcmpHeader, payload: &[u8], checksum: u16) -> Vec<u8> {
    let mut m = Vec::with_capacity(8 + payload.len());
    m.push(h.icmp_type);
    m.push(h.code);
    m.extend_from_slice(&checksum.to_be_bytes());
    m.extend_from_slice(&h.rest_of_header.to_be_bytes());
    m.extend_from_slice(payload);
    m
}

impl<P: Protocol> Protocol for Icmp<P> {
    fn protocol_id(&self) -> Option<u32> {
        Some(1) // ICMP's IP protocol number
    }

    fn layer(&self) -> Layer {
        // ICMP is a network-layer control protocol, but in the composition it occupies the
        // same slot UDP/TCP do — a checksummed payload of IP.
        Layer::Transport
    }

    fn encode_with(&self, ctx: &Context) -> Vec<u8> {
        let payload = self.payload.encode_with(ctx);
        // Self-contained checksum: over the header (checksum zeroed) + data, never a
        // pseudo-header — so it's computed unconditionally, regardless of `ctx`.
        let mut msg = message_bytes(&self.header, &payload, 0);
        let ck = internet_checksum(&msg);
        msg[2..4].copy_from_slice(&ck.to_be_bytes());
        msg
    }

    fn encode_raw_with(&self, ctx: &Context) -> Vec<u8> {
        let payload = self.payload.encode_raw_with(ctx);
        message_bytes(&self.header, &payload, self.header.checksum) // verbatim (forgeable)
    }
}
