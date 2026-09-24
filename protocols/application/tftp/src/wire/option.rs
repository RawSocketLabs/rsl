//! TFTP option-negotiation values (RFC 2347/2348/2349).
//!
//! Options ride as `name\0value\0` pairs at the end of a Read/Write request and,
//! once accepted, in an OACK. Names are matched case-insensitively. The three
//! standard options are modeled as typed variants; anything else is preserved
//! as [`TftpOption::Other`] so the codec never loses information and a caller
//! can negotiate experimental options.

use std::fmt::{self, Display, Formatter};

/// A single TFTP option as a typed name/value pair.
//~ models rfc2347#front
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TftpOption {
    /// `blksize` — negotiated data-block size in octets, 8..=65464 (RFC 2348).
    BlkSize(u16),
    /// `timeout` — retransmission timeout in seconds, 1..=255 (RFC 2349).
    Timeout(u8),
    /// `tsize` — transfer size in octets; a client requesting a read sends 0
    /// and the server answers with the real size (RFC 2349).
    TSize(u64),
    /// Any other option, kept verbatim (name lower-cased) for dual-use.
    Other {
        /// The option name (lower-cased).
        name: String,
        /// The option value, exactly as on the wire.
        value: String,
    },
}

impl TftpOption {
    /// The option's wire name (always lower-case).
    pub fn name(&self) -> &str {
        match self {
            TftpOption::BlkSize(_) => "blksize",
            TftpOption::Timeout(_) => "timeout",
            TftpOption::TSize(_) => "tsize",
            TftpOption::Other { name, .. } => name,
        }
    }

    /// The option's wire value as a string.
    pub fn value(&self) -> String {
        match self {
            TftpOption::BlkSize(n) => n.to_string(),
            TftpOption::Timeout(n) => n.to_string(),
            TftpOption::TSize(n) => n.to_string(),
            TftpOption::Other { value, .. } => value.clone(),
        }
    }

    /// Builds an option from a wire name/value pair, case-insensitively. A
    /// recognized name whose value parses becomes the typed variant; an
    /// unrecognized name — or a recognized name with an out-of-range or
    /// unparseable value — is preserved as [`Other`](Self::Other) rather than
    /// rejected, keeping the decoder liberal.
    pub fn from_pair(name: &str, value: &str) -> Self {
        let lower = name.to_ascii_lowercase();
        match lower.as_str() {
            "blksize" => match value.parse::<u16>() {
                // RFC 2348: valid blksize is 8..=65464.
                Ok(n) if (8..=65464).contains(&n) => TftpOption::BlkSize(n),
                _ => TftpOption::other(lower, value),
            },
            "timeout" => match value.parse::<u8>() {
                // RFC 2349: valid timeout is 1..=255 seconds.
                Ok(n) if (1..=255).contains(&n) => TftpOption::Timeout(n),
                _ => TftpOption::other(lower, value),
            },
            "tsize" => match value.parse::<u64>() {
                Ok(n) => TftpOption::TSize(n),
                _ => TftpOption::other(lower, value),
            },
            _ => TftpOption::other(lower, value),
        }
    }

    fn other(name: String, value: &str) -> Self {
        TftpOption::Other {
            name,
            value: value.to_string(),
        }
    }
}

impl Display for TftpOption {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}={}", self.name(), self.value())
    }
}

/// Looks up the negotiated `blksize` among a set of options, if present.
pub fn blksize(options: &[TftpOption]) -> Option<u16> {
    options.iter().find_map(|o| match o {
        TftpOption::BlkSize(n) => Some(*n),
        _ => None,
    })
}

/// Looks up the negotiated `timeout` (seconds) among a set of options.
pub fn timeout_secs(options: &[TftpOption]) -> Option<u8> {
    options.iter().find_map(|o| match o {
        TftpOption::Timeout(n) => Some(*n),
        _ => None,
    })
}

/// Looks up the `tsize` among a set of options.
pub fn tsize(options: &[TftpOption]) -> Option<u64> {
    options.iter().find_map(|o| match o {
        TftpOption::TSize(n) => Some(*n),
        _ => None,
    })
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn known_options_parse_to_typed_variants() {
        assert_eq!(
            TftpOption::from_pair("blksize", "1024"),
            TftpOption::BlkSize(1024)
        );
        assert_eq!(
            TftpOption::from_pair("BLKSIZE", "8"),
            TftpOption::BlkSize(8)
        );
        assert_eq!(
            TftpOption::from_pair("timeout", "5"),
            TftpOption::Timeout(5)
        );
        assert_eq!(TftpOption::from_pair("tsize", "0"), TftpOption::TSize(0));
    }

    #[test]
    fn out_of_range_or_unknown_is_preserved() {
        // blksize below the RFC 2348 floor is kept verbatim, not rejected.
        assert!(matches!(
            TftpOption::from_pair("blksize", "4"),
            TftpOption::Other { .. }
        ));
        assert_eq!(
            TftpOption::from_pair("X-Wild", "yes"),
            TftpOption::Other {
                name: "x-wild".to_string(),
                value: "yes".to_string()
            }
        );
    }

    #[test]
    fn lookups_find_typed_values() {
        let opts = vec![TftpOption::BlkSize(1432), TftpOption::Timeout(3)];
        assert_eq!(blksize(&opts), Some(1432));
        assert_eq!(timeout_secs(&opts), Some(3));
        assert_eq!(tsize(&opts), None);
    }
}
