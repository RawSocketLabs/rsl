//! Shared, version-neutral vocabulary for the SOCKS API.

mod destination;
mod version;

// --- Internal modules ---
pub use destination::Destination;
pub use version::Version;
