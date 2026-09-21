//! SOCKS5 wire types from RFC 1928 and RFC 1929.

mod codes;
mod domain;
mod endpoint;
mod method;
mod request;
mod username_password;
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
mod validation;
mod version;

pub use codes::{AddressType, AuthMethod, Command, ReplyCode};
pub use domain::{Domain, DomainError};
pub use endpoint::Endpoint;
pub use method::{MethodRequest, MethodSelection};
pub use request::{Reply, Request};
pub use username_password::{
    USERNAME_PASSWORD_VERSION, UsernamePasswordRequest, UsernamePasswordResponse,
    UsernamePasswordStatus,
};
pub use version::VERSION;
