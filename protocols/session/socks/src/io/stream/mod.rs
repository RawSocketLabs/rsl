//! Lossless buffered transport and backend-specific I/O implementations.

#[cfg(feature = "blocking")]
mod std;
mod stream;
#[cfg(feature = "tokio")]
mod tokio;

// --- Internal modules ---
pub(crate) use stream::MAX_FRAME_LEN;
pub use stream::Stream;
