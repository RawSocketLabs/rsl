// --- Internal modules ---
use super::super::{Builder as SettingsBuilder, Tokio};
use super::Client;
use crate::error::Error;

/// Build a tokio client using the common configuration validation.
pub type Builder = SettingsBuilder<Tokio>;

impl SettingsBuilder<Tokio> {
    /// Validate settings and produce a backend-specific reusable client without I/O.
    ///
    /// # Errors
    /// Rejects a zero or unrepresentable TCP setup budget.
    pub fn build(self) -> Result<Client, Error> {
        Ok(Client {
            settings: self.into_settings()?,
        })
    }
}
