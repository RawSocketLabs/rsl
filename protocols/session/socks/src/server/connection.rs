use crate::Stream;

/// Both sides of an established CONNECT session, after sending the success reply.
///
/// The client side retains all prefetched bytes in [`Stream`]; the target side is
/// the outbound connection. This is not itself a readable or writable stream.
/// Use `relay` for the supported TCP transports or [`Self::into_parts`] for custom
/// application I/O. Dropping it closes owned transports; no relay starts implicitly.
#[must_use = "relay the connection or transfer both streams to the application"]
pub struct Connection<S> {
    pub(crate) client: Stream<S>,
    pub(crate) target: S,
}

impl<S> Connection<S> {
    /// Transfer both connected sides, preserving the client's buffered prefix.
    pub fn into_parts(self) -> (Stream<S>, S) {
        (self.client, self.target)
    }
}
