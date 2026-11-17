//! Which end of the connection a value belongs to.

/// One end of a connection (RFC 5246 §6.1, `ConnectionEnd`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConnectionEnd {
    /// The end that sent the `ClientHello`.
    Client,
    /// The end that answered it.
    Server,
}
