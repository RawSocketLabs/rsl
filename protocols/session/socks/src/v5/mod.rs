//! SOCKS5 wire types from RFC 1928 and RFC 1929.

mod codes;
mod endpoint;
mod method;
mod request;
mod username_password;

pub use codes::{AddressType, AuthMethod, Command, ReplyCode};
pub use endpoint::Endpoint;
pub use method::{MethodRequest, MethodSelection};
pub use request::{Reply, Request};
pub use username_password::{
    USERNAME_PASSWORD_VERSION, UsernamePasswordRequest, UsernamePasswordResponse,
    UsernamePasswordStatus,
};

/// The SOCKS version byte used by RFC 1928 messages.
pub const VERSION: u8 = 5;
