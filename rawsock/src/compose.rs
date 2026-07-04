//! The layered packet-composition model.
//!
//! Stack protocols with each container's `.payload()` method (no operator overloading)
//! and [`encode`](ProtocolExt::encode) the result. Encoding is **lazy**: layers hold
//! their payload until `encode`, which walks the nest top-down handing each layer its
//! enclosing [`Context`] — that's what lets a cross-layer field (a UDP checksum needing
//! the IP pseudo-header) be computed.
//!
//! - [`ProtocolExt::encode`] — compliant: each layer computes its derived fields
//!   (lengths, checksums) when the `compute` feature is on (the default).
//! - [`ProtocolExt::encode_raw`] — verbatim: no computation, fields as built.
//!
//! A payload is *either* another [`Protocol`] (auto-sets the container's demux field) or
//! raw bytes (`Vec<u8>` is a [`Protocol`] leaf — the dual-use escape hatch, carries no
//! demux hint).

use crate::Layer;

/// Cross-layer context handed *down* to a payload during encoding.
#[derive(Clone, Default)]
pub struct Context {
    /// The enclosing L3 pseudo-header, supplied to L4 layers for their checksum.
    pub pseudo: Option<Pseudo>,
}

/// The IPv4 pseudo-header fields an L4 checksum covers.
#[derive(Clone, Copy, Debug)]
pub struct Pseudo {
    /// Source address that will appear in the IP header.
    pub src: std::net::Ipv4Addr,
    /// Destination address.
    pub dst: std::net::Ipv4Addr,
    /// IP protocol number (17 = UDP, 6 = TCP).
    pub protocol: u8,
}

/// A protocol layer: encodes a header + payload and reports a demux id.
pub trait Protocol {
    /// The id this layer presents to an enclosing demux field — UDP → 17 for IP's
    /// `Protocol`, IPv4 → 0x0800 for Ethernet's `EtherType`. `None` means no hint (raw
    /// bytes), so the container leaves its demux field as built.
    fn protocol_id(&self) -> Option<u32> {
        None
    }

    /// The tier this layer sits at, used for send routing of the outermost layer.
    fn layer(&self) -> Layer;

    /// Encode header + payload, computing derived fields from `ctx` (compliant).
    fn encode_with(&self, ctx: &Context) -> Vec<u8>;

    /// Encode verbatim — never computes derived fields (dual-use).
    fn encode_raw_with(&self, ctx: &Context) -> Vec<u8>;
}

/// Top-level encode helpers (start from an empty [`Context`]).
pub trait ProtocolExt: Protocol {
    /// Encode the whole nest, computing derived fields (the compliant default).
    fn encode(&self) -> Vec<u8> {
        self.encode_with(&Context::default())
    }
    /// Encode the whole nest verbatim — no computed fields (dual-use).
    fn encode_raw(&self) -> Vec<u8> {
        self.encode_raw_with(&Context::default())
    }
}
impl<T: Protocol + ?Sized> ProtocolExt for T {}

/// Opaque bytes as a leaf layer — the payload escape hatch. Carries no demux hint, so a
/// container given raw bytes keeps its demux field exactly as built.
impl Protocol for Vec<u8> {
    fn layer(&self) -> Layer {
        Layer::Transport
    }
    fn encode_with(&self, _ctx: &Context) -> Vec<u8> {
        self.clone()
    }
    fn encode_raw_with(&self, _ctx: &Context) -> Vec<u8> {
        self.clone()
    }
}

/// The one's-complement Internet checksum (RFC 1071) over `data`.
///
/// Returns the 16-bit checksum such that the sum of `data` (with this value in its
/// checksum slot) is all-ones. Verifying a buffer that already carries its checksum
/// yields `0`.
#[must_use]
pub fn internet_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut chunks = data.chunks_exact(2);
    for c in &mut chunks {
        sum += u32::from(u16::from_be_bytes([c[0], c[1]]));
    }
    if let [last] = chunks.remainder() {
        sum += u32::from(*last) << 8;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    // After the carry-fold loop `sum <= 0xFFFF`, so the narrowing is lossless.
    #[allow(clippy::cast_possible_truncation)]
    let folded = sum as u16;
    !folded
}
