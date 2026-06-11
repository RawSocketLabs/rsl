use std::fmt::{self, Display, Formatter};

/// A TFTP transfer mode — the netascii string that follows the filename in a
/// Read/Write request (RFC 1350 §1, §5).
///
/// Three modes are defined: `netascii`, `octet`, and `mail` (the last is
/// obsolete). The mode string is matched **case-insensitively**, so `OCTET`,
/// `Octet`, and `octet` are the same mode. Any other string is preserved as
/// [`Custom`](TransferMode::Custom) — the spec explicitly permits cooperating
/// hosts to define their own modes.
//~ models rfc1350#1 registry="mode"
//~ implements rfc1350#1/should-not.fa583c part="mail mode is modeled for fidelity but deliberately not driven"
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransferMode {
    /// `netascii` — 8-bit ASCII with the netascii line-ending conventions; the
    /// data is translated to/from the host's local text format.
    NetAscii,
    /// `octet` — raw 8-bit bytes, transferred verbatim.
    Octet,
    /// `mail` — obsolete; the "filename" names a mail recipient and the
    /// transfer must be a write. Modeled for completeness, not driven.
    Mail,
    /// Any other mode string (lower-cased for a known-unknown), preserved
    /// verbatim for dual-use.
    Custom(String),
}

impl TransferMode {
    /// The canonical wire string for this mode (always lower-case for the three
    /// defined modes).
    pub fn as_str(&self) -> &str {
        match self {
            TransferMode::NetAscii => "netascii",
            TransferMode::Octet => "octet",
            TransferMode::Mail => "mail",
            TransferMode::Custom(s) => s,
        }
    }

    /// Parses a mode from its wire bytes, case-insensitively. Unknown modes are
    /// kept (lossily decoded to a UTF-8 string) as [`Custom`](Self::Custom)
    /// rather than rejected.
    //~ implements rfc1350#5/must.5b9e67 part="accept cooperating-host modes"
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let text = String::from_utf8_lossy(bytes);
        match text.to_ascii_lowercase().as_str() {
            "netascii" => TransferMode::NetAscii,
            "octet" => TransferMode::Octet,
            "mail" => TransferMode::Mail,
            _ => TransferMode::Custom(text.into_owned()),
        }
    }

    /// Whether this is the `netascii` mode (the one that requires line-ending
    /// translation).
    pub fn is_netascii(&self) -> bool {
        matches!(self, TransferMode::NetAscii)
    }
}

impl Display for TransferMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn parsing_is_case_insensitive() {
        assert_eq!(TransferMode::from_bytes(b"netascii"), TransferMode::NetAscii);
        assert_eq!(TransferMode::from_bytes(b"OCTET"), TransferMode::Octet);
        assert_eq!(TransferMode::from_bytes(b"Mail"), TransferMode::Mail);
    }

    #[test]
    fn unknown_mode_is_preserved() {
        assert_eq!(
            TransferMode::from_bytes(b"x-custom"),
            TransferMode::Custom("x-custom".to_string())
        );
        assert_eq!(TransferMode::Octet.as_str(), "octet");
    }
}
