// --- Internal modules ---
use super::super::{Blocking, Builder as SettingsBuilder};
use super::Client;
use crate::error::Error;

/// Build a blocking client using the common configuration validation.
pub type Builder = SettingsBuilder<Blocking>;

impl SettingsBuilder<Blocking> {
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
