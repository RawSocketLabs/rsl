//! The master secret and the values derived from it.

// --- Workspace dependencies ---
use crypto::{CryptoError, SecretBytes};

// --- Internal modules ---
use super::{ConnectionEnd, prf_sha256};
use crate::record::{Aes128GcmProtection, TrafficKeys};

/// The 48-byte secret both ends derive from the key exchange, from which the record keys and
/// the `Finished` check values come (RFC 5246 §8.1).
///
/// The bytes are redacted from `Debug` output and zeroed on drop.
#[derive(Debug)]
pub struct MasterSecret(SecretBytes<{ Self::LEN }>);

impl MasterSecret {
    /// The master secret's length in bytes.
    pub const LEN: usize = 48;

    /// A master secret with exactly these bytes, for a caller that obtained them elsewhere
    /// (a key log, a resumed session).
    #[must_use]
    pub fn from_bytes(bytes: [u8; Self::LEN]) -> Self {
        Self(SecretBytes::new(bytes))
    }

    /// The master secret of a handshake that did not negotiate `extended_master_secret`:
    /// derived from the pre-master secret and the two hello randoms (RFC 5246 §8.1).
    ///
    /// # Errors
    ///
    /// Only when the pre-master secret is too long for SHA-256 to hash.
    //~ models rfc5246#8.1
    pub fn derive(
        pre_master_secret: &[u8],
        client_random: &[u8; 32],
        server_random: &[u8; 32],
    ) -> Result<Self, CryptoError> {
        let seed = [client_random.as_slice(), server_random].concat();
        Self::from_prf(pre_master_secret, b"master secret", &seed)
    }

    /// The master secret of a handshake that negotiated `extended_master_secret`: derived
    /// from the pre-master secret and `session_hash`, the hash of every handshake message up
    /// to and including the `ClientKeyExchange` (RFC 7627 §4).
    ///
    /// # Errors
    ///
    /// Only when an input is too long for SHA-256 to hash.
    //~ models rfc7627#4
    pub fn derive_extended(
        pre_master_secret: &[u8],
        session_hash: &[u8; 32],
    ) -> Result<Self, CryptoError> {
        Self::from_prf(pre_master_secret, b"extended master secret", session_hash)
    }

    /// The first [`Self::LEN`] bytes of `PRF(pre_master_secret, label, seed)`.
    fn from_prf(pre_master_secret: &[u8], label: &[u8], seed: &[u8]) -> Result<Self, CryptoError> {
        let mut bytes = SecretBytes::new([0; Self::LEN]);
        prf_sha256(pre_master_secret, label, seed, bytes.expose_secret_mut())?;
        Ok(Self(bytes))
    }

    /// The secret bytes, for a caller that must store or log them.
    #[must_use]
    pub fn expose_secret(&self) -> &[u8; Self::LEN] {
        self.0.expose_secret()
    }

    /// The check value `sender` puts in its `Finished` message, given the SHA-256 hash of
    /// every handshake message before that `Finished` (RFC 5246 §7.4.9).
    ///
    /// Compare a received value against this in constant time; see the crate's `DESIGN.md`
    /// §5 for why this type does not yet do that itself.
    ///
    /// # Errors
    ///
    /// Never in practice: the inputs are fixed-size and far below SHA-256's limit.
    //~ models rfc5246#7.4.9
    pub fn finished_verify_data(
        &self,
        sender: ConnectionEnd,
        handshake_hash: &[u8; 32],
    ) -> Result<[u8; 12], CryptoError> {
        let label: &[u8] = match sender {
            ConnectionEnd::Client => b"client finished",
            ConnectionEnd::Server => b"server finished",
        };
        let mut verify_data = [0; 12];
        prf_sha256(
            self.expose_secret(),
            label,
            handshake_hash,
            &mut verify_data,
        )?;
        Ok(verify_data)
    }

    /// The record keys for an AES-128-GCM suite: each direction's 16-byte key and 4-byte
    /// salt, cut in that order from the key block (RFC 5246 §6.3, RFC 5288 §3). An AEAD
    /// suite has no MAC keys, and this is the only key-block shape the crate implements.
    ///
    /// # Errors
    ///
    /// Only when the PRF's inputs are too long for SHA-256 to hash.
    //~ models rfc5246#6.3
    pub fn aes128_gcm_keys(
        &self,
        client_random: &[u8; 32],
        server_random: &[u8; 32],
    ) -> Result<TrafficKeys, CryptoError> {
        // Server random first: the reverse of the master-secret seed.
        let seed = [server_random.as_slice(), client_random].concat();
        let mut block = SecretBytes::new([0; 40]);
        prf_sha256(
            self.expose_secret(),
            b"key expansion",
            &seed,
            block.expose_secret_mut(),
        )?;
        let (keys, salts) = block.expose_secret().split_at(32);
        let (client_key, server_key) = keys.split_at(16);
        let (client_salt, server_salt) = salts.split_at(4);
        Ok(TrafficKeys {
            client_write: Aes128GcmProtection::from_slices(client_key, client_salt),
            server_write: Aes128GcmProtection::from_slices(server_key, server_salt),
        })
    }
}
