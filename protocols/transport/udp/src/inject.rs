//! The `rawsock` injection layer (the `inject` feature) — compose a UDP header with a
//! payload and encode it as a real, checksummed packet.
//!
//! [`Udp`] wraps a [`UdpHeader`](crate::UdpHeader) plus a payload and implements
//! [`rawsock::Protocol`], so it slots into rawsock's layered composition model. Encoding is
//! **dual-use**:
//! - [`encode`](rawsock::ProtocolExt::encode) (compliant) recomputes `length` from the
//!   payload and the checksum from the enclosing IPv4 pseudo-header (handed down in the
//!   [`Context`]); with no pseudo-header the checksum stays 0 (valid "no checksum" for IPv4).
//! - [`encode_raw`](rawsock::ProtocolExt::encode_raw) (verbatim) emits `.header`'s `length`
//!   and `checksum` exactly as set — forge them by writing the fields directly.

use crate::UdpHeader;
use rawsock::{Context, Layer, Protocol, Pseudo, internet_checksum};

/// A UDP layer for rawsock composition: a [`UdpHeader`](crate::UdpHeader) over a payload `P`
/// (another [`Protocol`] or raw `Vec<u8>`).
#[derive(Clone, Debug)]
pub struct Udp<P> {
    /// The UDP header. `length`/`checksum` are recomputed by the compliant `encode` and
    /// emitted verbatim by `encode_raw` — set them here to forge.
    pub header: UdpHeader,
    /// The encapsulated payload.
    pub payload: P,
}

impl<P> Udp<P> {
    /// A UDP layer from `src_port` → `dst_port` over `payload`, with `length`/`checksum`
    /// left 0 for the compliant `encode` to fill (set `.header` fields to forge them).
    pub fn new(src_port: u16, dst_port: u16, payload: P) -> Self {
        Self {
            header: UdpHeader {
                src_port,
                dst_port,
                length: 0,
                checksum: 0,
            },
            payload,
        }
    }
}

/// The UDP checksum (RFC 768) over the IPv4 pseudo-header + `udp` (the UDP header **with its
/// checksum field zeroed** followed by the payload). Returns the value to store — a computed
/// `0x0000` becomes `0xFFFF`, since `0` means "no checksum" on the wire for IPv4.
#[must_use]
pub fn udp_checksum(pseudo: &Pseudo, udp: &[u8]) -> u16 {
    let mut buf = Vec::with_capacity(12 + udp.len());
    buf.extend_from_slice(&pseudo.src.octets());
    buf.extend_from_slice(&pseudo.dst.octets());
    buf.push(0);
    buf.push(pseudo.protocol);
    // The pseudo-header's UDP-length field is the UDP datagram length (header + payload).
    buf.extend_from_slice(&u16::try_from(udp.len()).unwrap_or(u16::MAX).to_be_bytes());
    buf.extend_from_slice(udp);

    let ck = internet_checksum(&buf);
    if ck == 0 { 0xFFFF } else { ck }
}

impl<P: Protocol> Protocol for Udp<P> {
    fn protocol_id(&self) -> Option<u32> {
        Some(17) // UDP's IP protocol number
    }

    fn layer(&self) -> Layer {
        Layer::Transport
    }

    fn encode_with(&self, ctx: &Context) -> Vec<u8> {
        let payload = self.payload.encode_with(ctx);
        let length =
            u16::try_from(usize::from(UdpHeader::HEADER_LEN) + payload.len()).unwrap_or(u16::MAX);

        let mut udp = Vec::with_capacity(usize::from(UdpHeader::HEADER_LEN) + payload.len());
        udp.extend_from_slice(&self.header.src_port.to_be_bytes());
        udp.extend_from_slice(&self.header.dst_port.to_be_bytes());
        udp.extend_from_slice(&length.to_be_bytes());
        udp.extend_from_slice(&[0, 0]); // checksum computed below
        udp.extend_from_slice(&payload);

        // The UDP checksum needs the L3 pseudo-header. With none (a top-level encode), leave
        // it 0 — legal for IPv4.
        if let Some(pseudo) = ctx.pseudo {
            let ck = udp_checksum(&pseudo, &udp);
            udp[6..8].copy_from_slice(&ck.to_be_bytes());
        }
        udp
    }

    fn encode_raw_with(&self, ctx: &Context) -> Vec<u8> {
        let payload = self.payload.encode_raw_with(ctx);
        let mut udp = Vec::with_capacity(usize::from(UdpHeader::HEADER_LEN) + payload.len());
        udp.extend_from_slice(&self.header.src_port.to_be_bytes());
        udp.extend_from_slice(&self.header.dst_port.to_be_bytes());
        udp.extend_from_slice(&self.header.length.to_be_bytes()); // verbatim (forgeable)
        udp.extend_from_slice(&self.header.checksum.to_be_bytes()); // verbatim (forgeable)
        udp.extend_from_slice(&payload);
        udp
    }
}
