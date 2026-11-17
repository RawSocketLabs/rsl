//! The client's ephemeral ECDH key-exchange message.

// --- Workspace dependencies ---
use bnb::bin;

/// The body of a `ClientKeyExchange` for an ECDHE suite: the client's ephemeral public key
/// (RFC 8422 §5.7, `ClientECDiffieHellmanPublic`).
//~ models rfc8422#5.7
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ClientKeyExchange {
    /// The public key, in the same form as [`ServerEcdhParams::public`](super::ServerEcdhParams).
    #[brw(count_prefix = u8)]
    pub public: Vec<u8>,
}
