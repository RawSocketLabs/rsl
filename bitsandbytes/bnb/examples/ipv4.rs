//! **IPv4 header** — a real wire-format parser, end to end.
//!
//! One `#[bin]` message folds three `#[bitfield]`s and a `#[derive(BitEnum)]`, recomputes
//! the header checksum on write (`calc`), and maps the raw address words to real
//! `std::net::Ipv4Addr`s (`map`). It decodes a captured packet and logs exactly what came
//! in and what it became, then builds a fresh header with the required-by-default
//! **builder** — note we never supply a checksum; `#[builder(default)]` + `calc` fill it
//! in on write — and logs the bytes that come out. It also shows two dual-use escape
//! hatches: emitting a deliberately-wrong checksum (a `calc` passthrough toggled by a
//! `#[brw(ignore)]` field), and an unknown protocol number preserved rather than rejected.
//!
//! Output goes through **`tracing`** (a real logging facade), rendered by
//! `tracing-subscriber`. The header types are `no_std`-portable (decode from `&[u8]`,
//! encode to `Vec`); only this `main` needs `std`.
//!
//! Run with: `cargo run -p bitsandbytes --example ipv4`

use bnb::{BitEnum, bin, bitfield, u2, u4, u6, u13};
use std::net::Ipv4Addr;
use tracing::info;

// --- sub-byte structure, packed into byte-aligned bitfields ---------------------

/// First byte: 4-bit version + 4-bit header length (IHL), MSB-first.
#[bitfield(u8, bits = msb)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VersionIhl {
    version: u4,
    ihl: u4,
}

/// Type-of-service byte: a 6-bit DSCP class + a 2-bit ECN enum.
#[bitfield(u8, bits = msb)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Tos {
    dscp: u6,
    ecn: Ecn,
}

/// 2-bit ECN — a fully-covered enum (all four values), so it needs no `catch_all`.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
enum Ecn {
    NotEct,
    Ect1,
    Ect0,
    Ce,
}

/// The flags + 13-bit fragment offset, packed into one 16-bit word (MSB-first).
#[bitfield(u16, bits = msb)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FlagsFrag {
    reserved: bool, // the "evil bit" (RFC 3514), always 0
    dont_fragment: bool,
    more_fragments: bool,
    fragment_offset: u13,
}

/// IP protocol numbers. The `catch_all` preserves any value we don't name (dual-use);
/// the values are non-contiguous, so this needs an explicit `#[repr(u8)]`.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8)]
#[repr(u8)]
enum Protocol {
    Icmp = 1,
    Tcp = 6,
    Udp = 17,
    #[catch_all]
    Other(u8),
}

// --- the whole-message codec ----------------------------------------------------

/// A 20-byte IPv4 header (no options). `#[bin(big)]` because IP is network byte order.
#[bin(big)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Ipv4Header {
    ver_ihl: VersionIhl,
    tos: Tos,
    total_length: u16,
    identification: u16,
    flags_frag: FlagsFrag,
    ttl: u8,
    protocol: Protocol,
    // `calc` recomputes the checksum on every write (and `#[builder(default)]` means the
    // builder never asks for it). The conditional opts into a **dual-use passthrough**:
    // when `emit_raw_checksum` is set, write the stored `checksum` verbatim instead —
    // e.g. to emit a deliberately-wrong packet. (`self.checksum` is the stored value
    // because this field isn't `temp`.)
    #[bw(calc = if self.emit_raw_checksum { self.checksum } else { self.header_checksum() })]
    #[builder(default)]
    checksum: u16,
    // The wire repr is a big-endian `u32`; `map` turns it into a real `Ipv4Addr`.
    #[br(map = |raw: u32| Ipv4Addr::from(raw))]
    #[bw(map = |ip: &Ipv4Addr| u32::from(*ip))]
    src: Ipv4Addr,
    #[br(map = |raw: u32| Ipv4Addr::from(raw))]
    #[bw(map = |ip: &Ipv4Addr| u32::from(*ip))]
    dst: Ipv4Addr,
    /// Encode-time switch, **not on the wire** (`#[brw(ignore)]`): when `true`, the
    /// checksum `calc` above writes the stored value verbatim instead of recomputing —
    /// the dual-use escape hatch for emitting a non-compliant packet on purpose.
    /// `Default`s to `false` (recompute), including on decode.
    #[brw(ignore)]
    #[builder(default)]
    emit_raw_checksum: bool,
}

impl Ipv4Header {
    /// RFC 791 header checksum: the one's-complement of the one's-complement sum of the
    /// header's 16-bit words (the checksum field taken as zero). Computed from the
    /// fields — never by re-encoding, which would recurse back through `calc`.
    fn header_checksum(&self) -> u16 {
        let words = [
            (u16::from(self.ver_ihl.raw()) << 8) | u16::from(self.tos.raw()),
            self.total_length,
            self.identification,
            self.flags_frag.raw(),
            (u16::from(self.ttl) << 8) | u16::from(u8::from(self.protocol)),
            // (checksum word is zero for the computation)
            (u32::from(self.src) >> 16) as u16,
            u32::from(self.src) as u16,
            (u32::from(self.dst) >> 16) as u16,
            u32::from(self.dst) as u16,
        ];
        let mut sum: u32 = words.iter().map(|&w| u32::from(w)).sum();
        while (sum >> 16) != 0 {
            sum = (sum & 0xFFFF) + (sum >> 16);
        }
        !(sum as u16)
    }
}

/// Render bytes as space-separated hex for logging.
fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // A real logging subscriber renders the `tracing` events below to stderr.
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .without_time()
        .init();

    // ===== decode ===============================================================
    // The canonical RFC 791 / Wikipedia checksum example header (192.168.0.1 → .199, UDP).
    let wire: [u8; 20] = [
        0x45, 0x00, 0x00, 0x73, 0x00, 0x00, 0x40, 0x00, 0x40, 0x11, 0xb8, 0x61, 0xc0, 0xa8, 0x00,
        0x01, 0xc0, 0xa8, 0x00, 0xc7,
    ];
    info!(len = wire.len(), bytes = %hex(&wire), "decoding IPv4 header");

    let hdr = Ipv4Header::decode_exact(&wire)?;
    let computed = hdr.header_checksum();
    info!(
        version = %hdr.ver_ihl.version(),
        ihl = %hdr.ver_ihl.ihl(),
        total_length = hdr.total_length,
        id = hdr.identification,
        df = hdr.flags_frag.dont_fragment(),
        frag_offset = %hdr.flags_frag.fragment_offset(),
        ttl = hdr.ttl,
        protocol = ?hdr.protocol,
        src = %hdr.src,
        dst = %hdr.dst,
        checksum = %format!("0x{:04x}", hdr.checksum),
        checksum_valid = hdr.checksum == computed, // a stored field, so we can validate it
        "decoded header",
    );
    assert_eq!(hdr.checksum, computed);

    // Re-encode the decoded header — byte-for-byte identical (proves the checksum math).
    let reencoded = hdr.to_bytes()?;
    info!(bytes = %hex(&reencoded), "re-encoded the decoded header (expect byte-identical)");
    assert_eq!(reencoded, wire, "round-trip must be byte-identical");

    // ===== encode via the builder ===============================================
    // Build a fresh TCP header. We never call `.checksum(...)` — it's `#[builder(default)]`
    // and `calc` computes the real value on write.
    let header = Ipv4Header::builder()
        .ver_ihl(
            VersionIhl::new()
                .with_version(u4::new(4))
                .with_ihl(u4::new(5)),
        )
        .tos(Tos::new())
        .total_length(40)
        .identification(0x1c46)
        .flags_frag(FlagsFrag::new().with_dont_fragment(true))
        .ttl(64)
        .protocol(Protocol::Tcp)
        .src(Ipv4Addr::new(10, 0, 0, 1))
        .dst(Ipv4Addr::new(10, 0, 0, 2))
        .build()?;
    info!(
        protocol = ?header.protocol,
        src = %header.src,
        dst = %header.dst,
        checksum = header.checksum, // 0 — we never set it
        "header to write (checksum not set; calc will fill it)",
    );

    let bytes = header.to_bytes()?;
    let on_wire_checksum = u16::from_be_bytes([bytes[10], bytes[11]]);
    info!(
        len = bytes.len(),
        bytes = %hex(&bytes),
        checksum = %format!("0x{on_wire_checksum:04x}"),
        "encoded header (checksum auto-computed by calc)",
    );

    // The packet we just wrote decodes back, and its checksum validates.
    let parsed_back = Ipv4Header::decode_exact(&bytes)?;
    assert_eq!(parsed_back.checksum, parsed_back.header_checksum());

    // ===== dual-use: emit a deliberately-wrong checksum =========================
    // Same header, but flip the `emit_raw_checksum` switch and stash a bogus checksum.
    // `calc` then writes that stored value verbatim instead of recomputing — for
    // replaying a captured packet exactly, or testing whether a peer validates checksums.
    let mut tampered = header.clone();
    tampered.checksum = 0xBAD0; // a wrong value we want on the wire
    tampered.emit_raw_checksum = true; // opt into passthrough
    let tampered_bytes = tampered.to_bytes()?;
    let tampered_checksum = u16::from_be_bytes([tampered_bytes[10], tampered_bytes[11]]);
    info!(
        recomputed = %format!("0x{on_wire_checksum:04x}"),
        passthrough = %format!("0x{tampered_checksum:04x}"),
        "calc passthrough: default recomputes a correct checksum; emit_raw_checksum writes the stored value verbatim",
    );
    assert_ne!(on_wire_checksum, 0xBAD0); // default mode → recomputed, correct
    assert_eq!(tampered_checksum, 0xBAD0); // passthrough → the bogus stored value reached the wire

    // ===== dual-use: an unknown protocol is preserved ===========================
    let mut exotic = wire;
    exotic[9] = 0xFD; // an experimental protocol number not in our enum
    let parsed = Ipv4Header::decode_exact(&exotic)?;
    info!(protocol = ?parsed.protocol, "unknown protocol preserved, not rejected (catch_all)");
    assert_eq!(parsed.protocol, Protocol::Other(0xFD));

    info!("all checks passed");
    Ok(())
}
