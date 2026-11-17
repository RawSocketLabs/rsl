//! Bounds on what a decoder will hold.

/// Bounds on what a [`ResponseDecoder`](crate::ResponseDecoder) accepts. Every size is in
/// bytes. A response that exceeds one fails as soon as that is known, before the excess is
/// buffered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Limits {
    /// The longest head (status line and header fields) accepted.
    pub head: usize,
    /// The most header fields accepted.
    pub headers: usize,
    /// The longest body accepted. A `Content-Length` above it fails when the head arrives.
    pub body: usize,
    /// Whether a body that runs to the end of the connection is accepted when the transport
    /// could not confirm a clean close. Off by default: such a body cannot be told from a
    /// truncated one. Turn it on only for a server known to end connections that way.
    pub accept_unclean_close: bool,
}

impl Limits {
    /// These limits with another body limit.
    #[must_use]
    pub const fn with_body(mut self, body: usize) -> Self {
        self.body = body;
        self
    }

    /// These limits, accepting a read-to-close body after an unconfirmed close.
    #[must_use]
    pub const fn accepting_unclean_close(mut self) -> Self {
        self.accept_unclean_close = true;
        self
    }
}

impl Default for Limits {
    /// A 64 KiB head, 128 header fields, a 1 MiB body, and clean closes only.
    fn default() -> Self {
        Self {
            head: 64 * 1024,
            headers: 128,
            body: 1024 * 1024,
            accept_unclean_close: false,
        }
    }
}
