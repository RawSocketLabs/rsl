//! The composition model: `Context` threading, cross-layer compute, demux, and the
//! `encode` (compliant) vs `encode_raw` (verbatim) split.
//!
//! The real protocol crates (IP/UDP) that implement `Protocol` don't exist in this
//! workspace yet, so these tests stand up a minimal in-test stack to exercise the model
//! itself. When those crates land, `tests/` gains the concrete IPv4/UDP composition.

use rawsock::compose::internet_checksum;
use rawsock::{Context, Protocol, ProtocolExt, Pseudo};
use std::net::Ipv4Addr;

// A toy L4: two port bytes + payload + a trailing checksum that, when compliant,
// covers the enclosing pseudo-header (the cross-layer bit). Verbatim leaves it zero.
struct ToyL4 {
    port: u16,
    payload: Vec<u8>,
}

impl Protocol for ToyL4 {
    fn protocol_id(&self) -> Option<u32> {
        Some(17) // pretend-UDP
    }
    fn layer(&self) -> rawsock::Layer {
        rawsock::Layer::Transport
    }
    fn encode_with(&self, ctx: &Context) -> Vec<u8> {
        let mut out = self.port.to_be_bytes().to_vec();
        out.extend_from_slice(&self.payload);
        let mut covered = out.clone();
        if let Some(p) = ctx.pseudo {
            covered.extend_from_slice(&p.src.octets());
            covered.extend_from_slice(&p.dst.octets());
            covered.push(p.protocol);
        }
        out.extend_from_slice(&internet_checksum(&covered).to_be_bytes());
        out
    }
    fn encode_raw_with(&self, _ctx: &Context) -> Vec<u8> {
        let mut out = self.port.to_be_bytes().to_vec();
        out.extend_from_slice(&self.payload);
        out.extend_from_slice(&[0, 0]); // checksum not computed
        out
    }
}

// A toy L3: [protocol, len_hi, len_lo, ..body]. Auto-sets `protocol` from the payload's
// demux id (overridable), hands the payload a pseudo-header, and computes the length.
struct ToyL3 {
    src: Ipv4Addr,
    dst: Ipv4Addr,
    demux_override: Option<u8>,
    payload: ToyL4,
}

impl ToyL3 {
    fn protocol(&self) -> u8 {
        self.demux_override
            .or_else(|| self.payload.protocol_id().map(|id| id as u8))
            .unwrap_or(0)
    }
}

impl Protocol for ToyL3 {
    fn layer(&self) -> rawsock::Layer {
        rawsock::Layer::Network
    }
    fn encode_with(&self, _ctx: &Context) -> Vec<u8> {
        let proto = self.protocol();
        let child = Context {
            pseudo: Some(Pseudo {
                src: self.src,
                dst: self.dst,
                protocol: proto,
            }),
        };
        let body = self.payload.encode_with(&child);
        let mut out = vec![proto];
        out.extend_from_slice(&(body.len() as u16).to_be_bytes());
        out.extend_from_slice(&body);
        out
    }
    fn encode_raw_with(&self, _ctx: &Context) -> Vec<u8> {
        let body = self.payload.encode_raw_with(&Context::default());
        let mut out = vec![self.protocol(), 0, 0]; // length not computed
        out.extend_from_slice(&body);
        out
    }
}

#[test]
fn checksum_verifies_to_zero_when_in_place() {
    // A buffer whose last two bytes carry its own checksum sums to all-ones ⇒ 0.
    let mut buf = vec![0x45, 0x00, 0x00, 0x1c, 0xde, 0xad, 0x00, 0x00];
    let ck = internet_checksum(&buf);
    buf.extend_from_slice(&ck.to_be_bytes());
    assert_eq!(internet_checksum(&buf), 0);
}

#[test]
fn raw_bytes_are_a_verbatim_leaf() {
    let leaf = vec![1u8, 2, 3];
    assert_eq!(leaf.encode(), [1, 2, 3]);
    assert_eq!(leaf.encode_raw(), [1, 2, 3]);
    assert_eq!(Protocol::protocol_id(&leaf), None);
}

#[test]
fn demux_is_auto_set_from_the_payload() {
    let pkt = ToyL3 {
        src: Ipv4Addr::new(10, 0, 0, 1),
        dst: Ipv4Addr::new(10, 0, 0, 2),
        demux_override: None,
        payload: ToyL4 {
            port: 53,
            payload: vec![0xAA, 0xBB],
        },
    };
    let bytes = pkt.encode();
    assert_eq!(
        bytes[0], 17,
        "protocol auto-set from the payload's demux id"
    );
    // length = the L4 body: 2 port + 2 payload + 2 checksum = 6
    assert_eq!(u16::from_be_bytes([bytes[1], bytes[2]]), 6);
}

#[test]
fn demux_override_wins() {
    let pkt = ToyL3 {
        src: Ipv4Addr::new(1, 2, 3, 4),
        dst: Ipv4Addr::new(5, 6, 7, 8),
        demux_override: Some(6), // claim TCP while carrying pretend-UDP
        payload: ToyL4 {
            port: 1,
            payload: vec![0x00],
        },
    };
    assert_eq!(
        pkt.encode()[0],
        6,
        "override beats the payload's demux hint"
    );
}

#[test]
fn cross_layer_checksum_covers_the_pseudo_header() {
    let src = Ipv4Addr::new(10, 0, 0, 9);
    let dst = Ipv4Addr::new(10, 0, 0, 1);
    let pkt = ToyL3 {
        src,
        dst,
        demux_override: None,
        payload: ToyL4 {
            port: 40000,
            payload: vec![0x01, 0x02],
        },
    };
    let bytes = pkt.encode();
    let body = &bytes[3..]; // the L4 bytes, checksum in the trailing two
    let stored = u16::from_be_bytes([body[body.len() - 2], body[body.len() - 1]]);

    // Re-derive the checksum the encoder computed: L4 body (sans its own checksum) +
    // the pseudo-header the L3 layer handed down. Matching proves the cross-layer wiring.
    let mut covered = body[..body.len() - 2].to_vec();
    covered.extend_from_slice(&src.octets());
    covered.extend_from_slice(&dst.octets());
    covered.push(17);
    assert_eq!(stored, internet_checksum(&covered));
    assert_ne!(stored, 0, "the checksum was actually computed");
}

#[test]
fn encode_raw_skips_computed_fields() {
    let pkt = ToyL3 {
        src: Ipv4Addr::LOCALHOST,
        dst: Ipv4Addr::LOCALHOST,
        demux_override: None,
        payload: ToyL4 {
            port: 7,
            payload: vec![0xFF],
        },
    };
    let raw = pkt.encode_raw();
    assert_eq!(
        u16::from_be_bytes([raw[1], raw[2]]),
        0,
        "length not computed"
    );
    // last two bytes are the uncomputed L4 checksum
    assert_eq!(&raw[raw.len() - 2..], &[0, 0]);
    // demux is structural, not computed — still set
    assert_eq!(raw[0], 17);
}
