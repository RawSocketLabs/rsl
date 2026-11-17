//! What only a TCP tunnel connection uses (spec §9): the node's answer to a connect request,
//! and the frame header of the framed data phase.

mod frame;
mod reply;

pub use frame::{FrameHeader, FrameKind};
pub use reply::{ConnectReply, ReplyHead};
/// The result code of a connect request. The registry is SOCKS5's (spec §9.1).
pub use socks::v5::ReplyCode;
