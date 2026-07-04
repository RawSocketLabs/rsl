//! A structured **view** over the raw TCP option bytes (RFC 9293 §3.1, the option kinds from
//! the IANA registry).
//!
//! [`TcpHeader::options`](crate::TcpHeader::options) stays raw (dual-use: any/malformed
//! options are preserved exactly); [`parse`] is a lens that turns those bytes into typed
//! [`TcpOption`]s, and [`encode`] turns them back. Parsing is bounded and never panics on
//! hostile input — a malformed tail becomes [`TcpOption::Unknown`] and stops the scan.

/// One parsed TCP option. Common kinds are structured; anything else is preserved verbatim
/// as [`Unknown`](TcpOption::Unknown) (the dual-use rule — never lose representable data).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TcpOption {
    /// End of the option list (kind 0) — the scan stops after it.
    EndOfList,
    /// A no-op pad byte (kind 1), used to align a following option.
    Nop,
    /// Maximum segment size (kind 2, RFC 9293).
    Mss(u16),
    /// Window scale shift count (kind 3, RFC 7323).
    WindowScale(u8),
    /// SACK permitted (kind 4, RFC 2018).
    SackPermitted,
    /// Selective-ACK blocks (kind 5, RFC 2018): `(left_edge, right_edge)` pairs.
    Sack(Vec<(u32, u32)>),
    /// Timestamps (kind 8, RFC 7323): `TSval` and `TSecr`.
    Timestamps {
        /// The sender's timestamp value.
        tsval: u32,
        /// The echoed timestamp.
        tsecr: u32,
    },
    /// Any other (or malformed) option — its kind and raw value bytes, preserved.
    Unknown {
        /// The option kind byte.
        kind: u8,
        /// The value bytes (everything after the kind/length octets).
        value: Vec<u8>,
    },
}

const KIND_EOL: u8 = 0;
const KIND_NOP: u8 = 1;
const KIND_MSS: u8 = 2;
const KIND_WSCALE: u8 = 3;
const KIND_SACK_PERM: u8 = 4;
const KIND_SACK: u8 = 5;
const KIND_TIMESTAMPS: u8 = 8;

/// Parse raw TCP option bytes into a typed list.
///
/// EOL ends the list; NOP is a single pad byte; every other option is `kind, length, value`
/// where `length` counts the kind + length octets. A truncated or too-short option (a
/// `length` that runs past the buffer, or `< 2`) is preserved as [`TcpOption::Unknown`] with
/// the remaining bytes and stops the scan — decode of untrusted input never panics.
#[must_use]
pub fn parse(bytes: &[u8]) -> Vec<TcpOption> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let kind = bytes[i];
        match kind {
            KIND_EOL => {
                out.push(TcpOption::EndOfList);
                break;
            }
            KIND_NOP => {
                out.push(TcpOption::Nop);
                i += 1;
                continue;
            }
            _ => {}
        }
        // A length-bearing option needs at least a length octet, and it must not run past
        // the buffer. Anything else is a malformed tail — preserve it and stop.
        let Some(&len) = bytes.get(i + 1) else {
            out.push(TcpOption::Unknown {
                kind,
                value: bytes[i + 1..].to_vec(),
            });
            break;
        };
        let len = usize::from(len);
        if len < 2 || i + len > bytes.len() {
            out.push(TcpOption::Unknown {
                kind,
                value: bytes[i + 1..].to_vec(),
            });
            break;
        }
        let value = &bytes[i + 2..i + len];
        out.push(parse_one(kind, value));
        i += len;
    }
    out
}

fn parse_one(kind: u8, value: &[u8]) -> TcpOption {
    match (kind, value) {
        (KIND_MSS, [a, b]) => TcpOption::Mss(u16::from_be_bytes([*a, *b])),
        (KIND_WSCALE, [s]) => TcpOption::WindowScale(*s),
        (KIND_SACK_PERM, []) => TcpOption::SackPermitted,
        (KIND_SACK, v) if !v.is_empty() && v.len() % 8 == 0 => TcpOption::Sack(
            v.chunks_exact(8)
                .map(|c| {
                    (
                        u32::from_be_bytes([c[0], c[1], c[2], c[3]]),
                        u32::from_be_bytes([c[4], c[5], c[6], c[7]]),
                    )
                })
                .collect(),
        ),
        (KIND_TIMESTAMPS, [a, b, c, d, e, f, g, h]) => TcpOption::Timestamps {
            tsval: u32::from_be_bytes([*a, *b, *c, *d]),
            tsecr: u32::from_be_bytes([*e, *f, *g, *h]),
        },
        // Right kind but wrong length, or an unregistered kind — keep it verbatim.
        (kind, value) => TcpOption::Unknown {
            kind,
            value: value.to_vec(),
        },
    }
}

/// Encode a typed option list back to raw bytes (the inverse of [`parse`] for the structured
/// kinds). An [`Unknown`](TcpOption::Unknown) is emitted as `kind, length, value`.
#[must_use]
pub fn encode(options: &[TcpOption]) -> Vec<u8> {
    let mut out = Vec::new();
    for opt in options {
        match opt {
            TcpOption::EndOfList => out.push(KIND_EOL),
            TcpOption::Nop => out.push(KIND_NOP),
            TcpOption::Mss(mss) => {
                out.extend_from_slice(&[KIND_MSS, 4]);
                out.extend_from_slice(&mss.to_be_bytes());
            }
            TcpOption::WindowScale(s) => out.extend_from_slice(&[KIND_WSCALE, 3, *s]),
            TcpOption::SackPermitted => out.extend_from_slice(&[KIND_SACK_PERM, 2]),
            TcpOption::Sack(blocks) => {
                out.extend_from_slice(&[KIND_SACK, (2 + blocks.len() * 8) as u8]);
                for (l, r) in blocks {
                    out.extend_from_slice(&l.to_be_bytes());
                    out.extend_from_slice(&r.to_be_bytes());
                }
            }
            TcpOption::Timestamps { tsval, tsecr } => {
                out.extend_from_slice(&[KIND_TIMESTAMPS, 10]);
                out.extend_from_slice(&tsval.to_be_bytes());
                out.extend_from_slice(&tsecr.to_be_bytes());
            }
            TcpOption::Unknown { kind, value } => {
                out.push(*kind);
                out.push((value.len() + 2) as u8);
                out.extend_from_slice(value);
            }
        }
    }
    out
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn parses_the_common_kinds() {
        // MSS(1460), NOP, NOP, WScale(7), SACK-permitted, Timestamps, EOL.
        let raw = [
            0x02, 0x04, 0x05, 0xb4, // MSS 1460
            0x01, 0x01, // NOP NOP
            0x03, 0x03, 0x07, // WScale 7
            0x04, 0x02, // SACK permitted
            0x08, 0x0a, 0, 0, 0, 1, 0, 0, 0, 2,    // Timestamps tsval=1 tsecr=2
            0x00, // EOL
        ];
        let opts = parse(&raw);
        assert_eq!(
            opts,
            vec![
                TcpOption::Mss(1460),
                TcpOption::Nop,
                TcpOption::Nop,
                TcpOption::WindowScale(7),
                TcpOption::SackPermitted,
                TcpOption::Timestamps { tsval: 1, tsecr: 2 },
                TcpOption::EndOfList,
            ]
        );
    }

    #[test]
    fn sack_blocks_round_trip() {
        let opts = vec![TcpOption::Sack(vec![(100, 200), (300, 400)])];
        let raw = encode(&opts);
        assert_eq!(parse(&raw), opts);
    }

    #[test]
    fn unknown_and_wrong_length_are_preserved() {
        // Kind 99 (unregistered), then a kind-2 (MSS) with the wrong length (3, not 4).
        let raw = [0x63, 0x03, 0xaa, 0x02, 0x03, 0xbb];
        assert_eq!(
            parse(&raw),
            vec![
                TcpOption::Unknown {
                    kind: 99,
                    value: vec![0xaa]
                },
                TcpOption::Unknown {
                    kind: 2,
                    value: vec![0xbb]
                },
            ]
        );
    }

    #[test]
    fn a_length_past_the_buffer_is_preserved_not_panicked() {
        // Kind 2 claims length 4 but only 2 bytes remain — preserved as Unknown, no panic.
        let raw = [0x02, 0x04, 0xaa];
        assert_eq!(
            parse(&raw),
            vec![TcpOption::Unknown {
                kind: 2,
                value: vec![0x04, 0xaa]
            }]
        );
    }

    #[test]
    fn encode_is_the_inverse_for_structured_kinds() {
        let opts = vec![
            TcpOption::Mss(536),
            TcpOption::WindowScale(2),
            TcpOption::SackPermitted,
            TcpOption::Nop,
            TcpOption::EndOfList,
        ];
        assert_eq!(parse(&encode(&opts)), opts);
    }
}
