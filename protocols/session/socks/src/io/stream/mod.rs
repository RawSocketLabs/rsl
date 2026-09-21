//! Lossless buffered transport and backend-specific I/O implementations.

#[cfg(any(feature = "blocking", feature = "mio"))]
mod std;
mod stream;
#[cfg(feature = "tokio")]
mod tokio;

pub(crate) use stream::MAX_FRAME_LEN;
pub use stream::Stream;
