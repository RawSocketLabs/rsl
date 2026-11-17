//! Who a tunnel is opened for.

// --- Standard library ---
use std::fmt;

// --- Workspace dependencies ---
use crypto::RandomSource;
use sangfor_atrust_sig::{SignKey, identifiers};

// --- Internal modules ---
use super::Error;

/// What identifies a client to a node, the same for every stream of one client setup
/// (spec §9.2): the session, the user, the device, and a key the client makes up.
pub struct Identity {
    /// The session identifier.
    sid: String,
    /// The user's name as the gateway gave it. It may be empty.
    user_name: String,
    /// The device id.
    device_id: String,
    /// The connection id, computed once from the device id and the time of setup.
    connection_id: String,
    /// The key that signs tunnel requests. It never leaves the client.
    sign_key: SignKey,
}

impl Identity {
    /// The identity of a client set up at `unix_micros` (microseconds since the Unix epoch),
    /// with a fresh random sign key.
    ///
    /// # Errors
    ///
    /// [`Error::Sig`] when `random` fails.
    pub fn new(
        sid: impl Into<String>,
        user_name: impl Into<String>,
        device_id: impl Into<String>,
        unix_micros: u64,
        random: &mut impl RandomSource,
    ) -> Result<Self, Error> {
        let key = SignKey::generate(random)?;
        Self::with_sign_key(sid, user_name, device_id, unix_micros, key)
    }

    /// Like [`new`](Self::new), with a sign key the caller supplies: one a deployment
    /// configures, or a fixed one in a test.
    ///
    /// # Errors
    ///
    /// [`Error::Sig`] only if `device_id` is too long to hash.
    pub fn with_sign_key(
        sid: impl Into<String>,
        user_name: impl Into<String>,
        device_id: impl Into<String>,
        unix_micros: u64,
        sign_key: SignKey,
    ) -> Result<Self, Error> {
        let device_id = device_id.into();
        Ok(Self {
            sid: sid.into(),
            user_name: user_name.into(),
            connection_id: identifiers::connection_id(&device_id, unix_micros)?,
            device_id,
            sign_key,
        })
    }

    /// Replace the session identifier, after a refresh issued a new one (spec §5.8). Streams
    /// opened from then on use it.
    pub fn set_sid(&mut self, sid: impl Into<String>) {
        self.sid = sid.into();
    }

    /// The session identifier.
    pub(super) fn sid(&self) -> &str {
        &self.sid
    }

    /// The user's name.
    pub(super) fn user_name(&self) -> &str {
        &self.user_name
    }

    /// The device id.
    pub(super) fn device_id(&self) -> &str {
        &self.device_id
    }

    /// The connection id.
    #[must_use]
    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }

    /// The key that signs tunnel requests.
    pub(super) const fn sign_key(&self) -> &SignKey {
        &self.sign_key
    }
}

impl fmt::Debug for Identity {
    /// Shows whose identity it is, and neither the session identifier nor the key.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Identity")
            .field("user_name", &self.user_name)
            .field("device_id", &self.device_id)
            .field("connection_id", &self.connection_id)
            .finish_non_exhaustive()
    }
}
