//! The handshake message-type registry.

// --- Workspace dependencies ---
use bnb::BitEnum;

/// Which handshake message follows a [`HandshakeHeader`](super::HandshakeHeader)
/// (RFC 5246 §7.4). Dual-use: any other value is preserved as [`HandshakeType::Other`].
//~ models rfc5246#7.4
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[bit_enum(u8)]
#[repr(u8)]
pub enum HandshakeType {
    /// The server asks the client to start a new handshake.
    HelloRequest = 0,
    /// The client's opening offer.
    ClientHello = 1,
    /// The server's choice from that offer.
    ServerHello = 2,
    /// A certificate chain.
    Certificate = 11,
    /// The server's ephemeral key-exchange parameters.
    ServerKeyExchange = 12,
    /// The server asks for a client certificate.
    CertificateRequest = 13,
    /// The end of the server's hello flight.
    ServerHelloDone = 14,
    /// The client's proof that it holds its certificate's key.
    CertificateVerify = 15,
    /// The client's key-exchange contribution.
    ClientKeyExchange = 16,
    /// The check over the whole handshake.
    Finished = 20,
    /// Any other message type, preserved verbatim.
    #[catch_all]
    Other(u8),
}
