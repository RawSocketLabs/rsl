//! The `rawsock` injection layer (the `inject` feature) — wrap an L4 payload in IPv4 and
//! encode the full datagram.
//!
//! [`Ip`] is the piece that makes a whole stack correct: on the compliant
//! [`encode`](rawsock::ProtocolExt::encode) it hands its payload the IPv4 **pseudo-header**
//! (source/destination/protocol) in a child [`Context`], so the L4 layer (UDP/TCP) can compute
//! its checksum — then it fills its own `total_length` and `protocol` from the payload and
//! computes the IPv4 **header checksum**. [`encode_raw`](rawsock::ProtocolExt::encode_raw)
//! emits everything verbatim (dual-use).

use crate::Ipv4Header;
use rawsock::{Context, Layer, Protocol, Pseudo, internet_checksum};

/// An IPv4 layer for rawsock composition: an [`Ipv4Header`](crate::Ipv4Header) over an L4
/// payload `P`.
#[derive(Clone, Debug)]
pub struct Ip<P> {
    /// The IPv4 header. `total_length`, `protocol`, and `header_checksum` are recomputed by the
    /// compliant `encode` and emitted verbatim by `encode_raw` — set them to forge.
    pub header: Ipv4Header,
    /// The encapsulated payload (a transport layer, or raw bytes).
    pub payload: P,
}

impl<P> Ip<P> {
    /// An IPv4 layer wrapping `header` over `payload`. Build `header` with
    /// [`Ipv4Header::datagram`](crate::Ipv4Header::datagram) (or a struct literal to forge).
    pub fn new(header: Ipv4Header, payload: P) -> Self {
        Self { header, payload }
    }
}

/// The header bytes of `h` (fixed 20 + options), used for the checksum and the wire.
fn header_bytes(h: &Ipv4Header) -> Vec<u8> {
    let mut b = Vec::with_capacity(h.header_len().max(20));
    b.extend_from_slice(&h.version_ihl.to_be_bytes());
    b.push(h.dscp_ecn);
    b.extend_from_slice(&h.total_length.to_be_bytes());
    b.extend_from_slice(&h.identification.to_be_bytes());
    b.extend_from_slice(&h.flags_fragment.to_be_bytes());
    b.push(h.ttl);
    b.push(h.protocol);
    b.extend_from_slice(&h.header_checksum.to_be_bytes());
    b.extend_from_slice(&h.src.octets());
    b.extend_from_slice(&h.dst.octets());
    b.extend_from_slice(&h.options);
    b
}

impl<P: Protocol> Protocol for Ip<P> {
    fn protocol_id(&self) -> Option<u32> {
        Some(0x0800) // IPv4's EtherType, for an enclosing Ethernet demux
    }

    fn layer(&self) -> Layer {
        Layer::Network
    }

    fn encode_with(&self, _ctx: &Context) -> Vec<u8> {
        // The transport layer's demux id is its IP protocol number; fall back to the header's
        // if the payload gives no hint (raw bytes).
        let protocol = self
            .payload
            .protocol_id()
            .and_then(|id| u8::try_from(id).ok())
            .unwrap_or(self.header.protocol);

        // Hand the payload the pseudo-header so its checksum covers our addresses + protocol.
        let child = Context {
            pseudo: Some(Pseudo {
                src: self.header.src,
                dst: self.header.dst,
                protocol,
            }),
        };
        let payload = self.payload.encode_with(&child);

        // Fill the derived header fields, then checksum the header (with the field zeroed).
        let mut header = self.header.clone();
        header.protocol = protocol;
        header.total_length =
            u16::try_from(header.header_len() + payload.len()).unwrap_or(u16::MAX);
        header.header_checksum = 0;

        let mut out = header_bytes(&header);
        let ck = internet_checksum(&out);
        out[10..12].copy_from_slice(&ck.to_be_bytes());
        out.extend_from_slice(&payload);
        out
    }

    fn encode_raw_with(&self, _ctx: &Context) -> Vec<u8> {
        // Verbatim all the way down — no pseudo-header, no recomputation.
        let payload = self.payload.encode_raw_with(&Context::default());
        let mut out = header_bytes(&self.header);
        out.extend_from_slice(&payload);
        out
    }
}
