//! Private operation failures: protocol facts plus the executor's unchanged transport error.

/// A pure machine's failure, interpreted by its executor rather than an OS error wrapper.
/// `E` is the native transport error or a `no_std` caller's own error; no bound requires std.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Failure<E> {
    /// The caller-supplied elapsed time reached the operation deadline.
    #[cfg(feature = "usb")]
    Timeout,
    /// The request or cleanup mode is outside the machine's supported capabilities.
    #[cfg(feature = "usb")]
    Unsupported(&'static str),
    /// Malformed data or an invalid operation/event/clock transition.
    InvalidData(&'static str),
    /// A correlated transport failure, retained without allocation or error erasure.
    Transport(E),
}
