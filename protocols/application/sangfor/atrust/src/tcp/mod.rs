//! The TCP tunnel (spec §9): one TCP stream to a host behind the gateway, carried over one
//! connection to a node.
//!
//! The protocol parts do no I/O. [`Destination::route`] decides, from the policy, whether a
//! destination goes through a tunnel and with which values. [`Handshake`] builds what a
//! client sends when the connection opens and reads the node's answer from bytes as they
//! arrive. [`FrameReader`] and [`frame`] are the framed data phase.
//!
//! [`Stream`] drives them over a [`Transport`], which is a TLS connection to a node: a byte
//! stream that can stop writing while it goes on reading.

mod destination;
mod error;
mod frames;
mod handshake;
mod identity;
mod request;
mod stream;

pub use destination::Destination;
pub use error::Error;
pub use frames::{FrameReader, MAX_FRAME_DATA, frame};
pub use handshake::{Handshake, Mode, Progress};
pub use identity::Identity;
pub use stream::{Stream, Transport};
