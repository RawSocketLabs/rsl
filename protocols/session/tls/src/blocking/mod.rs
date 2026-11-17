//! A blocking TLS stream: the [`Client`](crate::Client) driven over any `Read + Write`
//! transport, such as a `TcpStream`.

mod error;
mod stream;

pub use error::Error;
pub use stream::Stream;
