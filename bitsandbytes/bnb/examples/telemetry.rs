//! **Telemetry packet** — a synthetic frame that stress-tests bnb's *composition*: three
//! levels of nested bitfields, mixed sub-byte widths, two `BitEnum`s (one closed, one
//! `catch_all`), a `#[bitflags]` set, a `#[reserved]` field, a `count`-driven sample
//! array, a `calc`'d CRC, and a construction-side `validate`. It also exercises the two
//! encode paths (verbatim `to_bytes` vs canonical `to_canonical_bytes`) and the canonical
//! helpers, and shows the required-by-default builder catching mistakes.
//!
//! Not a real protocol — the point is to show the pieces working together. Output goes
//! through `tracing`; the message types are `no_std`-portable.
//!
//! Run with: `cargo run -p bitsandbytes --example telemetry`

use bnb::{BitEnum, bin, bitfield, bitflags, u2, u3, u4, u5, u7};
use tracing::info;

// --- the nesting: Quality (level 2) inside Header (level 1) inside the frame ----

/// Signal quality — an 8-bit bitfield, packed **LSB-first** (the `bit_order` variant).
#[bitfield(u8, bits = lsb)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Quality {
    snr: u5,        // signal-to-noise ratio (5 bits)
    confidence: u3, // 0..7 (3 bits)
}

/// 2-bit message priority — all four values are named, so it needs neither `catch_all`
/// nor `closed`.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
enum Priority {
    Low,
    Normal,
    High,
    Critical,
}

/// 4-bit sensor kind — the `catch_all` preserves an unknown kind (dual-use). Contiguous
/// from 0, so the derive auto-numbers and no `#[repr]` is needed.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u4)]
enum SensorKind {
    Temperature,
    Pressure,
    Humidity,
    Voltage,
    #[catch_all]
    Unknown(u4),
}

/// The 32-bit header word: a spread of odd widths plus the nested `Quality` bitfield and
/// two nested enums. MSB-first (the RFC-diagram convention).
#[bitfield(u32, bits = msb)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Header {
    version: u3,        // 3
    priority: Priority, // 2 (enum)
    kind: SensorKind,   // 4 (enum, catch_all)
    quality: Quality,   // 8 (nested bitfield — level 2)
    channels: u7,       // 7
    seq: u8,            // 8  → 3+2+4+8+7+8 = 32
}

/// Per-frame status flags (a `#[bitflags]` set over one byte).
#[bitflags(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Status {
    calibrated: bool,
    fault: bool,
    low_battery: bool,
    retransmit: bool,
}

// --- the whole-message frame ---------------------------------------------------

/// `#[bin(big)]` with a construction-side `validate`. The CRC is a `calc` field (so
/// `to_bytes` is verbatim and `to_canonical_bytes` recomputes it), and `rsv` is reserved
/// (retained verbatim, normalized canonically) — so this type also gets the canonical
/// helpers.
#[bin(big, validate = frame_is_sound)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct TelemetryFrame {
    header: Header,
    status: Status,
    #[reserved]
    rsv: u8, // must-be-zero padding byte (spec value 0)
    #[br(temp)]
    #[bw(calc = self.samples.len() as u8)]
    count: u8, // not stored — derived from `samples`
    #[br(count = count)]
    samples: Vec<u16>,
    #[bw(calc = self.crc())]
    #[builder(default)]
    crc: u16,
}

impl TelemetryFrame {
    /// A toy frame checksum (illustrative — not a standard CRC). Folds the samples with
    /// the status byte; computed from the fields, never by re-encoding.
    fn crc(&self) -> u16 {
        let mut c: u16 = 0xFFFF ^ u16::from(self.status.bits());
        for &s in &self.samples {
            c = c.rotate_left(7) ^ s;
        }
        c
    }
}

/// Construction-side soundness (gates `build()`, never the parser): a known protocol
/// version and at least one sample.
fn frame_is_sound(f: &TelemetryFrame) -> Result<(), String> {
    if f.header.version() == u3::new(0) {
        return Err("protocol version must be >= 1".into());
    }
    if f.samples.is_empty() {
        return Err("a frame must carry at least one sample".into());
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .without_time()
        .init();

    // ===== build a frame with the required-by-default builder ====================
    let header = Header::new()
        .with_version(u3::new(1))
        .with_priority(Priority::High)
        .with_kind(SensorKind::Pressure)
        .with_quality(
            Quality::new()
                .with_snr(u5::new(27))
                .with_confidence(u3::new(6)),
        )
        .with_channels(u7::new(4))
        .with_seq(0x2A);
    let frame = TelemetryFrame::builder()
        .header(header)
        .status(Status::CALIBRATED | Status::LOW_BATTERY)
        .samples(vec![0x1001, 0x1002, 0x1003])
        .build()?; // rsv defaults to spec (0); crc defaults; count is temp
    info!(
        ?header,
        "built header (3-level nesting; bitfield Debug decomposes it)"
    );

    // The CRC is unset (builder default 0), so the frame isn't canonical yet.
    info!(
        is_canonical = frame.is_canonical(),
        diff = ?frame.canonical_diff(), // ["crc"]
        "freshly built frame",
    );
    assert!(!frame.is_canonical());
    assert_eq!(frame.canonical_diff(), ["crc"]);

    // ===== encode: verbatim vs canonical ========================================
    let verbatim = frame.to_bytes()?; // writes the stored crc (0) and stored rsv
    let canonical = frame.to_canonical_bytes()?; // recomputes crc, normalizes rsv
    info!(
        verbatim_len = verbatim.len(),
        canonical_crc = %format!("0x{:04x}", frame.crc()),
        "to_bytes is verbatim (crc=0); to_canonical_bytes fills the crc in",
    );
    // A decoded frame is verbatim; canonicalizing makes it round-trip to the canonical bytes.
    let decoded = TelemetryFrame::decode_exact(&canonical)?;
    info!(
        version = %decoded.header.version(),
        priority = ?decoded.header.priority(),
        kind = ?decoded.header.kind(),
        snr = %decoded.header.quality().snr(),
        samples = decoded.samples.len(),
        is_canonical = decoded.is_canonical(),
        "decoded the canonical frame",
    );
    assert!(decoded.is_canonical());
    assert_eq!(decoded.samples, vec![0x1001, 0x1002, 0x1003]);
    assert_eq!(decoded.to_bytes()?, canonical); // verbatim round-trip of a canonical value

    // ===== reserved: retained verbatim, normalized canonically ==================
    let mut tampered = decoded.clone();
    tampered.rsv = 0xFF; // a peer set non-spec reserved bits
    // Wire layout: header(4) status(1) rsv(1) count(1) samples… — so rsv is byte 5.
    assert_eq!(tampered.to_bytes()?[5], 0xFF); // verbatim keeps them
    assert_eq!(tampered.to_canonical_bytes()?[5], 0x00); // canonical zeroes them
    info!(diff = ?tampered.canonical_diff(), "rsv set to 0xFF → not canonical");

    // ===== the builder catches mistakes =========================================
    // Forget a required field → `build()` names the first missing one.
    let missing = TelemetryFrame::builder()
        .header(header)
        .samples(vec![0x1])
        .build(); // never set `status`
    info!(error = %missing.unwrap_err(), "builder rejects a missing required field");

    // `validate` gates `build()` (but never the parser): version 0 is unsound.
    let bad = TelemetryFrame::builder()
        .header(Header::new().with_version(u3::new(0)))
        .status(Status::empty())
        .samples(vec![0x1])
        .build();
    assert!(bad.is_err());
    info!(error = %bad.unwrap_err(), "validate rejects an unsound frame");

    info!("all checks passed");
    Ok(())
}
