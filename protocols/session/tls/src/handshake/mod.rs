//! The handshake layer's plumbing: joining messages out of records and hashing them.

mod joiner;
mod message;
mod transcript;

pub use joiner::{HandshakeJoiner, MessageTooLong};
pub use message::HandshakeMessage;
pub use transcript::Transcript;
