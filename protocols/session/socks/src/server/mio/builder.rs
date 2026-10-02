// --- Internal modules ---
use super::super::{Builder as SettingsBuilder, Mio};
use super::Server;
use crate::error::Error;

/// Backend-selected server builder; shared options remain available.
pub type Builder = SettingsBuilder<Mio>;

impl SettingsBuilder<Mio> {
    /// Validate configuration without binding a listener or performing I/O.
    ///
    /// # Errors
    /// Rejects missing policy, empty protocols, or invalid resource limits.
    pub fn build(self) -> Result<Server, Error> {
        Ok(Server {
            config: self.into_config()?,
        })
    }
}
