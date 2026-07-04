//! The `rawsock` injection layer (the `inject` feature) — make an [`ArpPacket`](crate::ArpPacket)
//! a `rawsock::Protocol` so it can be framed and injected.
//!
//! ARP is a **leaf**: a complete message with no payload, no checksum, and no length field — so
//! there are no derived fields, and `encode` == `encode_raw` (everything is verbatim). It
//! presents EtherType `0x0806` to an enclosing Ethernet frame.

use crate::ArpPacket;
use rawsock::{Context, Layer, Protocol};

/// ARP's EtherType (`0x0806`), the demux id it presents to an enclosing Ethernet frame.
const ARP_ETHERTYPE: u32 = 0x0806;

impl Protocol for ArpPacket {
    fn protocol_id(&self) -> Option<u32> {
        Some(ARP_ETHERTYPE)
    }

    fn layer(&self) -> Layer {
        Layer::Network
    }

    fn encode_with(&self, _ctx: &Context) -> Vec<u8> {
        // A fixed 28-byte packet with no derived fields — encoding cannot fail.
        self.to_bytes()
            .expect("a fixed-layout ARP packet always encodes")
    }

    fn encode_raw_with(&self, ctx: &Context) -> Vec<u8> {
        self.encode_with(ctx) // nothing to recompute — verbatim either way
    }
}
