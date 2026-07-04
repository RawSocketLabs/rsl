//! The `rawsock` injection layer (the `inject` feature) — compose a TCP header with a payload
//! and encode it as a real, checksummed segment.
//!
//! [`Tcp`] wraps a [`TcpHeader`](crate::TcpHeader) plus a payload and implements
//! [`rawsock::Protocol`]. Encoding is **dual-use**:
//! - [`encode`](rawsock::ProtocolExt::encode) (compliant) computes the checksum from the
//!   enclosing IPv4 pseudo-header (handed down in the [`Context`]); with no pseudo-header it
//!   stays as built. TCP has no length field of its own (the IP layer carries it).
//! - [`encode_raw`](rawsock::ProtocolExt::encode_raw) (verbatim) emits `.header`'s `checksum`
//!   exactly as set — forge it by writing the field.

use crate::TcpHeader;
use rawsock::{Context, Layer, Protocol, Pseudo, internet_checksum};

/// A TCP layer for rawsock composition: a [`TcpHeader`](crate::TcpHeader) over a payload `P`
/// (another [`Protocol`] or raw `Vec<u8>`).
#[derive(Clone, Debug)]
pub struct Tcp<P> {
    /// The TCP header. `checksum` is recomputed by the compliant `encode` and emitted verbatim
    /// by `encode_raw` — set it here to forge.
    pub header: TcpHeader,
    /// The encapsulated payload (segment data).
    pub payload: P,
}

impl<P> Tcp<P> {
    /// A TCP layer wrapping `header` over `payload`. Build `header` with
    /// [`TcpHeader::segment`](crate::TcpHeader::segment) (or the struct literal to forge).
    pub fn new(header: TcpHeader, payload: P) -> Self {
        Self { header, payload }
    }
}

/// The bytes of `header` with the checksum field zeroed, followed by `payload`.
fn segment_bytes(header: &TcpHeader, payload: &[u8], checksum: u16) -> Vec<u8> {
    let mut tcp = Vec::with_capacity(20 + header.options.len() + payload.len());
    tcp.extend_from_slice(&header.src_port.to_be_bytes());
    tcp.extend_from_slice(&header.dst_port.to_be_bytes());
    tcp.extend_from_slice(&header.seq.to_be_bytes());
    tcp.extend_from_slice(&header.ack.to_be_bytes());
    tcp.extend_from_slice(&header.control.to_be_bytes());
    tcp.extend_from_slice(&header.window.to_be_bytes());
    tcp.extend_from_slice(&checksum.to_be_bytes());
    tcp.extend_from_slice(&header.urgent.to_be_bytes());
    tcp.extend_from_slice(&header.options);
    tcp.extend_from_slice(payload);
    tcp
}

/// The TCP checksum (RFC 9293 §3.1) over the IPv4 pseudo-header + `tcp` (the TCP header **with
/// its checksum field zeroed** followed by the segment data). Unlike UDP there is no
/// "no checksum" sentinel — a computed `0x0000` is stored as-is.
#[must_use]
pub fn tcp_checksum(pseudo: &Pseudo, tcp: &[u8]) -> u16 {
    let mut buf = Vec::with_capacity(12 + tcp.len());
    buf.extend_from_slice(&pseudo.src.octets());
    buf.extend_from_slice(&pseudo.dst.octets());
    buf.push(0);
    buf.push(pseudo.protocol);
    // The pseudo-header's length field is the TCP segment length (header + data).
    buf.extend_from_slice(&u16::try_from(tcp.len()).unwrap_or(u16::MAX).to_be_bytes());
    buf.extend_from_slice(tcp);
    internet_checksum(&buf)
}

impl<P: Protocol> Protocol for Tcp<P> {
    fn protocol_id(&self) -> Option<u32> {
        Some(6) // TCP's IP protocol number
    }

    fn layer(&self) -> Layer {
        Layer::Transport
    }

    fn encode_with(&self, ctx: &Context) -> Vec<u8> {
        let payload = self.payload.encode_with(ctx);
        // Build with the checksum zeroed, then fill it from the pseudo-header if present.
        let mut tcp = segment_bytes(&self.header, &payload, 0);
        if let Some(pseudo) = ctx.pseudo {
            let ck = tcp_checksum(&pseudo, &tcp);
            tcp[16..18].copy_from_slice(&ck.to_be_bytes());
        }
        tcp
    }

    fn encode_raw_with(&self, ctx: &Context) -> Vec<u8> {
        let payload = self.payload.encode_raw_with(ctx);
        segment_bytes(&self.header, &payload, self.header.checksum) // verbatim (forgeable)
    }
}
