//! One request and its response over a blocking `Read + Write` transport.

mod error;
mod exchange;

pub use error::Error;
pub use exchange::exchange;
