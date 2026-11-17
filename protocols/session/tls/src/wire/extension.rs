//! Hello extensions.

// --- Workspace dependencies ---
use bnb::{BitEncode, BitEnum, BitError, BitWriter, bin};

// --- Internal modules ---
use super::{NamedGroup, SignatureScheme, list::write_list16};

/// Which extension an [`Extension`] carries. Only the extensions this crate builds or acts on
/// are named; any other value is preserved as [`ExtensionType::Other`].
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[bit_enum(u16, bytes = big)]
#[repr(u16)]
pub enum ExtensionType {
    /// `server_name` (RFC 6066 §3).
    ServerName = 0,
    /// `supported_groups`, originally `elliptic_curves` (RFC 8422 §5.1.1).
    SupportedGroups = 10,
    /// `ec_point_formats` (RFC 8422 §5.1.2).
    EcPointFormats = 11,
    /// `signature_algorithms` (RFC 5246 §7.4.1.4.1).
    SignatureAlgorithms = 13,
    /// `extended_master_secret` (RFC 7627 §5.1).
    ExtendedMasterSecret = 0x0017,
    /// `renegotiation_info` (RFC 5746 §3.2).
    RenegotiationInfo = 0xff01,
    /// Any other extension, preserved verbatim.
    #[catch_all]
    Other(u16),
}

/// One hello extension: a type and its opaque data (RFC 5246 §7.4.1.4).
///
/// The data is kept as raw bytes, so an extension this crate does not understand, or one a
/// caller wants to malform, round-trips unchanged. The constructors build the compliant
/// encodings of the extensions a client sends.
//~ models rfc5246#7.4.1.4
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Extension {
    /// Which extension this is.
    pub extension_type: ExtensionType,
    /// The extension's data, uninterpreted.
    #[brw(count_prefix = u16)]
    pub data: Vec<u8>,
}

/// One entry of a `server_name` list: a name type and a name (RFC 6066 §3).
#[bin(big)]
struct ServerName {
    name_type: u8,
    #[brw(count_prefix = u16)]
    name: Vec<u8>,
}

impl Extension {
    /// An extension with exactly this type and data (no validation).
    #[must_use]
    pub const fn new(extension_type: ExtensionType, data: Vec<u8>) -> Self {
        Self {
            extension_type,
            data,
        }
    }

    /// `server_name` naming one DNS host (RFC 6066 §3).
    ///
    /// # Errors
    ///
    /// When `host_name` is too long for its 16-bit length.
    pub fn server_name(host_name: &str) -> Result<Self, BitError> {
        /// `NameType` `host_name` (RFC 6066 §3).
        const HOST_NAME: u8 = 0;
        let entry = ServerName {
            name_type: HOST_NAME,
            name: host_name.as_bytes().to_vec(),
        };
        list16(ExtensionType::ServerName, &[entry])
    }

    /// `supported_groups` offering `groups`, most preferred first (RFC 8422 §5.1.1).
    ///
    /// # Errors
    ///
    /// When the list is too long for its 16-bit length.
    pub fn supported_groups(groups: &[NamedGroup]) -> Result<Self, BitError> {
        list16(ExtensionType::SupportedGroups, groups)
    }

    /// `ec_point_formats` offering only the uncompressed format (RFC 8422 §5.1.2).
    #[must_use]
    pub fn ec_point_formats_uncompressed() -> Self {
        /// `ECPointFormat` `uncompressed` (RFC 8422 §5.1.2).
        const UNCOMPRESSED: u8 = 0;
        Self::new(ExtensionType::EcPointFormats, vec![1, UNCOMPRESSED])
    }

    /// `signature_algorithms` offering `schemes`, most preferred first
    /// (RFC 5246 §7.4.1.4.1).
    ///
    /// # Errors
    ///
    /// When the list is too long for its 16-bit length.
    pub fn signature_algorithms(schemes: &[SignatureScheme]) -> Result<Self, BitError> {
        list16(ExtensionType::SignatureAlgorithms, schemes)
    }

    /// `extended_master_secret`, whose data is empty (RFC 7627 §5.1).
    #[must_use]
    pub const fn extended_master_secret() -> Self {
        Self::new(ExtensionType::ExtendedMasterSecret, Vec::new())
    }

    /// `renegotiation_info` for an initial handshake: an empty `renegotiated_connection`
    /// (RFC 5746 §3.2).
    #[must_use]
    pub fn renegotiation_info_initial() -> Self {
        Self::new(ExtensionType::RenegotiationInfo, vec![0])
    }
}

/// An extension whose data is one list behind a 16-bit byte length.
fn list16<T: BitEncode>(extension_type: ExtensionType, items: &[T]) -> Result<Extension, BitError> {
    let mut data = BitWriter::new();
    write_list16(items, &mut data)?;
    Ok(Extension::new(extension_type, data.into_bytes()))
}
