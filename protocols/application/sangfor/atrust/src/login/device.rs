//! How the client identifies its device.

// --- Workspace dependencies ---
use crypto::RandomSource;
use sangfor_atrust_sig::identifiers;

// --- Internal modules ---
use super::Error;

/// The identity a client reports for its device (spec §3.8). A gateway may bind an account
/// to it, so a client keeps the same one across logins.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Device {
    /// The device id: 32 lowercase hex digits.
    pub id: String,
    /// The device's MAC addresses, lowercase with colons. A login reports them, and needs at
    /// least one.
    pub mac_addresses: Vec<String>,
}

impl Device {
    /// A fresh identity: a random id and one random, locally administered MAC address.
    ///
    /// # Errors
    ///
    /// [`Error::Sig`] when `random` fails.
    pub fn generate(random: &mut impl RandomSource) -> Result<Self, Error> {
        Ok(Self {
            id: identifiers::device_id(random)?,
            mac_addresses: vec![identifiers::mac_address(random)?],
        })
    }
}
