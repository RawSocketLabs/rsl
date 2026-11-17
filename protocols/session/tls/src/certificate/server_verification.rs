//! How a client decides whether to trust a server's chain.

// --- Workspace dependencies ---
use pki::{PathValidator, Purpose, RequiredKeyUsage};
use x509::{Certificate, Time};

// --- Internal modules ---
use super::{CertificateError, PeerChain};

/// The client's policy for the server's certificate chain.
///
/// There is no default: the caller chooses between validating and explicitly not validating.
#[derive(Clone, Copy, Debug)]
pub enum ServerVerification<'a> {
    /// Validate the chain with [`pki`]: a path from the leaf to one of `trust_anchors`, valid
    /// at `time`, with a leaf whose `subjectAltName` matches `dns_name` and whose
    /// `extendedKeyUsage`, if present, allows server authentication.
    Path {
        /// Certificates the caller trusts as roots.
        trust_anchors: &'a [Certificate<'a>],
        /// The instant validity periods are checked against. The caller supplies it because
        /// this crate reads no clock.
        time: Time,
        /// The server name the leaf must be valid for, in ASCII or IDNA A-label form.
        dns_name: &'a str,
    },
    /// Accept the chain without validating it against any trust anchor, name or time.
    ///
    /// The connection is then open to an active attacker who presents a certificate of their
    /// own. It exists for gateways that present certificates no trust store validates, and
    /// for dual-use testing. [`verify`](Self::verify) itself accepts anything under this
    /// policy, even an empty or unparseable chain. The handshake is stricter: the
    /// [`Client`](crate::Client) still requires the server to sign its key exchange with the
    /// leaf certificate's key, so the leaf must parse and the peer must hold its key. That
    /// makes the chain the caller reads afterwards the one the peer can actually speak for.
    Unverified,
}

impl ServerVerification<'_> {
    /// Applies this policy to the chain the server presented.
    ///
    /// `key_usage` is what the negotiated key exchange needs from the leaf's key (RFC 5246
    /// §7.4.2): a signature for ECDHE, key encipherment for static RSA. The handshake passes
    /// it because it follows from the cipher suite, not from the caller's policy.
    ///
    /// # Errors
    ///
    /// Under [`ServerVerification::Path`]: [`CertificateError::EmptyChain`],
    /// [`CertificateError::Parse`], or [`CertificateError::Path`] with [`pki`]'s reason.
    /// [`ServerVerification::Unverified`] never fails.
    pub fn verify(
        &self,
        chain: &PeerChain,
        key_usage: RequiredKeyUsage,
    ) -> Result<(), CertificateError> {
        // Exhaustive on purpose: a new variant must say what it checks, never inherit "accept".
        let (trust_anchors, time, dns_name) = match *self {
            Self::Unverified => return Ok(()),
            Self::Path {
                trust_anchors,
                time,
                dns_name,
            } => (trust_anchors, time, dns_name),
        };
        let certificates = chain.parse()?;
        let (leaf, intermediates) = certificates
            .split_first()
            .ok_or(CertificateError::EmptyChain)?;
        PathValidator::builder()
            .leaf(leaf)
            .trust_anchors(trust_anchors)
            .at_time(time)
            .intermediates(intermediates)
            .purpose(Purpose::ServerAuth)
            .required_key_usage(key_usage)
            .dns_name(dns_name)
            .validate()
            .map(|_| ())
            .map_err(CertificateError::Path)
    }
}
