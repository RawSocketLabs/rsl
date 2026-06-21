//! **IPv4 header** — a real wire-format parser, end to end.
//!
//! One `#[bin]` message folds three `#[bitfield]`s and a `#[derive(BitEnum)]`, and maps the
//! raw address words to real `std::net::Ipv4Addr`s (`map`). The header checksum is a `calc`
//! field, which makes it the showcase for bnb's two encode paths:
//!   * **`to_bytes()` is verbatim** — it writes exactly what's stored, so a decoded packet
//!     round-trips byte-identically, and a deliberately-wrong checksum goes on the wire as-is
//!     (dual-use);
//!   * **`to_canonical_bytes()` is canonical** — it recomputes the checksum, so the result is
//!     always valid.
//!
//! It also uses the canonical helpers (`is_canonical`, `canonical_diff`, `to_canonical`) and
//! shows an unknown protocol number preserved rather than rejected. Output goes through
//! **`tracing`**. The header types are `no_std`-portable; only this `main` needs `std`.
//!
//! Run with: `cargo run -p bitsandbytes --example ipv4`

use bnb::{BitEnum, EncodeExt, EncodeMode, bin, bitfield, u2, u4, u6, u13};
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
    // A `calc` field: `to_bytes` writes the stored value verbatim, `to_canonical_bytes`
    // recomputes it. `#[builder(default)]` means the builder never asks for it (canonicalize
    // to fill it in). Having a non-`temp` `calc` field is what makes bnb generate
    // `to_canonical_bytes` / `is_canonical` / `canonical_diff` / `to_canonical` for this type.
    #[bw(calc = self.header_checksum())]
    #[builder(default)]
    checksum: u16,
    // The wire repr is a big-endian `u32`; `map` turns it into a real `Ipv4Addr`.
    #[br(map = |raw: u32| Ipv4Addr::from(raw))]
    #[bw(map = |ip: &Ipv4Addr| u32::from(*ip))]
    src: Ipv4Addr,
    #[br(map = |raw: u32| Ipv4Addr::from(raw))]
    #[bw(map = |ip: &Ipv4Addr| u32::from(*ip))]
    dst: Ipv4Addr,
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

/// The checksum bytes (offset 10..12) of an encoded header.
fn checksum_of(bytes: &[u8]) -> u16 {
    u16::from_be_bytes([bytes[10], bytes[11]])
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .without_time()
        .init();

    // ===== decode (always verbatim) =============================================
    // The canonical RFC 791 / Wikipedia checksum example header (192.168.0.1 → .199, UDP).
    let wire: [u8; 20] = [
        0x45, 0x00, 0x00, 0x73, 0x00, 0x00, 0x40, 0x00, 0x40, 0x11, 0xb8, 0x61, 0xc0, 0xa8, 0x00,
        0x01, 0xc0, 0xa8, 0x00, 0xc7,
    ];
    info!(len = wire.len(), bytes = %hex(&wire), "decoding IPv4 header");

    let hdr = Ipv4Header::decode_exact(&wire)?;
    info!(
        // the bitfield `Debug` decomposes `ver_ihl`/`flags_frag` into logical fields
        version = %hdr.ver_ihl.version(),
        ihl = %hdr.ver_ihl.ihl(),
        ttl = hdr.ttl,
        protocol = ?hdr.protocol,
        df = hdr.flags_frag.dont_fragment(),
        src = %hdr.src,
        dst = %hdr.dst,
        checksum = %format!("0x{:04x}", hdr.checksum),
        is_canonical = hdr.is_canonical(), // a valid packet is already canonical
        "decoded header",
    );
    assert!(hdr.is_canonical());
    assert!(hdr.canonical_diff().is_empty());

    // `to_bytes` is verbatim, so a decoded packet round-trips byte-for-byte.
    let verbatim = hdr.to_bytes()?;
    info!(bytes = %hex(&verbatim), "to_bytes (verbatim) → byte-identical to the input");
    assert_eq!(verbatim, wire);

    // ===== build + canonicalize =================================================
    // The builder never sets a checksum (it's `#[builder(default)]`, so it defaults to 0).
    let built = Ipv4Header::builder()
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
        stored_checksum = built.checksum,            // 0 — never set
        is_canonical = built.is_canonical(),         // false: 0 ≠ the real checksum
        diff = ?built.canonical_diff(),              // ["checksum"]
        "built a header (checksum unset)",
    );
    assert!(!built.is_canonical());

    // `to_canonical_bytes` recomputes the checksum, giving a valid packet on the wire.
    let canonical = built.to_canonical_bytes()?;
    info!(
        bytes = %hex(&canonical),
        checksum = %format!("0x{:04x}", checksum_of(&canonical)),
        "to_canonical_bytes → checksum filled in",
    );

    // ===== dual-use: verbatim vs canonical ======================================
    // A header carrying a deliberately-wrong checksum. `to_bytes` emits it as-is (for
    // replay / testing a peer); `to_canonical_bytes` corrects it. No special flag needed —
    // the choice is which method you call.
    let mut tampered = built.clone();
    tampered.checksum = 0xBAD0;
    let raw = tampered.to_bytes()?; // verbatim
    let fixed = tampered.to_canonical_bytes()?; // canonical
    info!(
        diff = ?tampered.canonical_diff(),
        verbatim = %format!("0x{:04x}", checksum_of(&raw)),
        canonical = %format!("0x{:04x}", checksum_of(&fixed)),
        "to_bytes keeps 0xBAD0; to_canonical_bytes recomputes the real value",
    );
    assert_eq!(checksum_of(&raw), 0xBAD0);
    assert_ne!(checksum_of(&fixed), 0xBAD0);
    // `to_canonical()` is the same correction, in memory.
    assert!(tampered.clone().to_canonical().is_canonical());

    // ===== runtime mode selection: encode(w, mode) ==============================
    // When verbatim-vs-canonical is a *runtime* value (a config flag, a CLI option),
    // `encode(w, mode)` writes straight to any std::io::Write — here a Vec standing in
    // for a socket. (`to_bytes`/`to_canonical_bytes` are the compile-time form.)
    for mode in [EncodeMode::Verbatim, EncodeMode::Canonical] {
        let mut socket: Vec<u8> = Vec::new();
        tampered.encode(&mut socket, mode)?;
        info!(?mode, checksum = %format!("0x{:04x}", checksum_of(&socket)), "encode(w, mode)");
    }

    // ===== dual-use: an unknown protocol is preserved ===========================
    let mut exotic = wire;
    exotic[9] = 0xFD; // an experimental protocol number not in our enum
    let parsed = Ipv4Header::decode_exact(&exotic)?;
    info!(protocol = ?parsed.protocol, "unknown protocol preserved, not rejected (catch_all)");
    assert_eq!(parsed.protocol, Protocol::Other(0xFD));

    info!("all checks passed");
    Ok(())
}
