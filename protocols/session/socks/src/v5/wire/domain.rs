use bnb::bin;
use thiserror::Error;

const MAX_LEN: usize = u8::MAX as usize;

/// A domain-length violation in compliant SOCKS5 construction.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum DomainError {
    /// The domain name contains no bytes.
    #[error("domain name must contain at least one byte")]
    Empty,
    /// The domain name does not fit the one-octet length prefix.
    #[error("domain name length {length} exceeds {MAX_LEN}")]
    TooLong {
        /// The supplied byte length.
        length: usize,
    },
}

/// A SOCKS5 domain endpoint payload: length-prefixed name followed by its port.
///
/// This payload excludes `ATYP`; [`super::Endpoint::Domain`] supplies that byte.
/// The builder requires 1–255 name bytes. Decoding and direct field construction remain
/// permissive; encoding rejects lengths that cannot fit the one-octet prefix.
/// Names remain opaque bytes: no UTF-8, DNS-label, IDNA, or resolution policy is applied.
///
/// ```
/// use socks::v5::{Domain, Endpoint};
///
/// let domain = Domain::builder()
///     .name(b"example.com".to_vec())
///     .port(443)
///     .build()?;
/// let endpoint = Endpoint::from(domain);
/// assert_eq!(endpoint.port(), 443);
/// # Ok::<(), bnb::BuilderError>(())
/// ```
//~ models rfc1928#4 part="domain address and port"
#[bin(big, validate = Domain::check_length)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Domain {
    /// The domain name bytes, without an added terminator or normalization.
    #[brw(count_prefix = u8)]
    pub name: Vec<u8>,
    /// The network port.
    pub port: u16,
}

impl Domain {
    /// Check the name length without changing or interpreting its bytes.
    ///
    /// Used by the builder and safe to repeat after raw construction, decoding, or mutation.
    ///
    /// # Errors
    /// Returns [`DomainError::Empty`] or [`DomainError::TooLong`] for lengths outside 1–255.
    pub fn check_length(&self) -> Result<(), DomainError> {
        match self.name.len() {
            0 => Err(DomainError::Empty),
            1..=MAX_LEN => Ok(()),
            length => Err(DomainError::TooLong { length }),
        }
    }
}
