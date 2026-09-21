/// A protocol supported by this crate's guided client and server.
/// Only SOCKS5 is implemented; no implicit version fallback is performed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Version {
    /// SOCKS5 (RFC 1928), with optional RFC 1929 authentication.
    V5,
}
