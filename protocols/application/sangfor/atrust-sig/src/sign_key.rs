//! The key the client signs with, and its signatures.

// --- Workspace dependencies ---
use crypto::{
    RandomSource, SecretBytes, digest::sha2::sha256::Sha256, mac::hmac::sha256::HmacSha256,
};

// --- Internal modules ---
use crate::{Error, hex};

/// Mixed into the sign key. Embedded in the vendor client (spec §3.1, `C_key`).
const KEY_CONSTANT: &[u8] = b"3uW5IEy8KwDaOMK8uw1TmNr50U3aK1Qdu8b6vopXxGstzan3AJXxVNR6piuKi5Nq";

/// A 32-byte HMAC-SHA-256 key and the protocol's way of signing with it: the tag as 64
/// uppercase hexadecimal digits (spec §3.3).
///
/// Two keys in the protocol have this shape: the one derived from every `authConfig` response,
/// which signs control-plane requests and checks the server's answers, and the tunnel sign
/// key, which the client makes up.
#[derive(Debug)]
pub struct SignKey(SecretBytes<32>);

impl SignKey {
    /// A key with exactly these bytes.
    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(SecretBytes::new(bytes))
    }

    /// A new random key, as a client makes for signing its tunnel requests (spec §3.8).
    ///
    /// # Errors
    ///
    /// [`Error::Crypto`] when `random` fails.
    pub fn generate(random: &mut impl RandomSource) -> Result<Self, Error> {
        let mut key = SecretBytes::new([0; 32]);
        random.fill_bytes(key.expose_secret_mut())?;
        Ok(Self(key))
    }

    /// The key an `authConfig` response establishes, from three strings of its
    /// `antiMITMAttackData`, each used as text exactly as received. When the response carries
    /// no such object all three are empty, and a key is still derived.
    ///
    /// # Errors
    ///
    /// Only if an input is too long for SHA-256 to hash.
    pub fn derive(
        device_pub_key_mod: &str,
        device_pub_key_exp: &str,
        challenge: &str,
    ) -> Result<Self, Error> {
        let mut first = Sha256::new();
        first.update(device_pub_key_mod)?;
        first.update(device_pub_key_exp)?;
        first.update(KEY_CONSTANT)?;
        let first = first.finalize().into_bytes();

        let mut second = Sha256::new();
        second.update(hex::encode(&first, true))?;
        second.update(challenge)?;
        let second = second.finalize().into_bytes();

        let mut key = SecretBytes::new([0; 32]);
        for (byte, (a, b)) in key
            .expose_secret_mut()
            .iter_mut()
            .zip(first.iter().zip(second))
        {
            *byte = a ^ b;
        }
        Ok(Self(key))
    }

    /// The key's bytes, for a caller that must store them.
    #[must_use]
    pub fn expose_secret(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }

    /// The signature of `message`: its HMAC-SHA-256 tag as 64 uppercase hexadecimal digits.
    ///
    /// # Errors
    ///
    /// Only if `message` is too long for SHA-256 to hash.
    pub fn sign(&self, message: &[u8]) -> Result<String, Error> {
        let tag = HmacSha256::authenticate(self.expose_secret(), message)?;
        Ok(hex::encode(tag.as_bytes(), true))
    }

    /// Whether `signature` is this key's signature of `message`. Letter case is ignored, as
    /// the protocol uppercases a received signature before comparing; an empty signature
    /// never passes. The comparison does not stop at the first difference.
    ///
    /// # Errors
    ///
    /// Only if `message` is too long for SHA-256 to hash.
    pub fn verify(&self, message: &[u8], signature: &str) -> Result<bool, Error> {
        let expected = crypto::SecretVec::new(self.sign(message)?.into_bytes());
        Ok(expected.matches(signature.to_ascii_uppercase()))
    }
}
