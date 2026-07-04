//! The `rawsock` injection layer (the `inject` feature) — frame an L3 payload in Ethernet for
//! L2 injection.
//!
//! [`Ethernet`] wraps an [`EthernetHeader`](crate::EthernetHeader) plus a payload and
//! implements [`rawsock::Protocol`] at [`Layer::Link`] — the **top** of the stack (it presents
//! no demux id upward). On [`encode`](rawsock::ProtocolExt::encode) it sets the frame's
//! EtherType from the payload's demux id (IPv4 → `0x0800`, ARP → `0x0806`). There is no
//! checksum here: the 4-byte FCS is computed by the NIC on transmit.

use crate::EthernetHeader;
use ethertype::EtherType;
use rawsock::{Context, Layer, Protocol};

/// An Ethernet layer for rawsock composition: an [`EthernetHeader`](crate::EthernetHeader) over
/// an L3 payload `P` (an IP datagram, an ARP message, or raw bytes).
#[derive(Clone, Debug)]
pub struct Ethernet<P> {
    /// The frame header. `ethertype` is set from the payload's demux id by the compliant
    /// `encode` and emitted verbatim by `encode_raw` — set it to forge.
    pub header: EthernetHeader,
    /// The framed payload.
    pub payload: P,
}

impl<P> Ethernet<P> {
    /// An Ethernet frame wrapping `header` over `payload`.
    pub fn new(header: EthernetHeader, payload: P) -> Self {
        Self { header, payload }
    }
}

/// The 14-byte frame header (dst, src, ethertype).
fn header_bytes(h: &EthernetHeader) -> Vec<u8> {
    let mut b = Vec::with_capacity(EthernetHeader::HEADER_LEN);
    b.extend_from_slice(&h.dst);
    b.extend_from_slice(&h.src);
    b.extend_from_slice(&u16::from(h.ethertype).to_be_bytes());
    b
}

impl<P: Protocol> Protocol for Ethernet<P> {
    fn protocol_id(&self) -> Option<u32> {
        None // L2 frame — the outermost layer, nothing demuxes it further up
    }

    fn layer(&self) -> Layer {
        Layer::Link
    }

    fn encode_with(&self, ctx: &Context) -> Vec<u8> {
        let payload = self.payload.encode_with(ctx);
        let mut header = self.header;
        // Set the EtherType from the payload's demux id (an EtherType value, e.g. IPv4/ARP).
        if let Some(id) = self.payload.protocol_id() {
            if let Ok(et) = u16::try_from(id) {
                header.ethertype = EtherType::from(et);
            }
        }
        let mut out = header_bytes(&header);
        out.extend_from_slice(&payload);
        out // no FCS — the NIC computes and appends it on transmit
    }

    fn encode_raw_with(&self, ctx: &Context) -> Vec<u8> {
        let payload = self.payload.encode_raw_with(ctx);
        let mut out = header_bytes(&self.header); // verbatim EtherType (forgeable)
        out.extend_from_slice(&payload);
        out
    }
}
