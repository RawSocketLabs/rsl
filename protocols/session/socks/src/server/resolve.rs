//! Shared input preparation for server-side system resolvers.
// --- Internal modules ---
use crate::error::Error;

/// Borrow a nonempty UTF-8 name without NULs for system resolution, not DNS/IDNA validation.
pub(crate) fn domain_name(bytes: &[u8]) -> Result<&str, Error> {
    let name = std::str::from_utf8(bytes).map_err(|_| Error::InvalidEndpoint)?;
    if name.is_empty() || name.contains('\0') {
        Err(Error::InvalidEndpoint)
    } else {
        Ok(name)
    }
}
