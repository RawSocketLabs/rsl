//! Alert messages.

// --- Workspace dependencies ---
use bnb::{BitEnum, bin};

/// How serious an alert is (RFC 5246 §7.2). Dual-use: any other value is preserved.
//~ models rfc5246#7.2
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[bit_enum(u8)]
#[repr(u8)]
pub enum AlertLevel {
    /// The connection may continue.
    Warning = 1,
    /// The connection ends now.
    Fatal = 2,
    /// Any other level, preserved verbatim.
    #[catch_all]
    Other(u8),
}

/// What an alert reports (RFC 5246 §7.2). Dual-use: any other value is preserved.
//~ models rfc5246#7.2
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[bit_enum(u8)]
#[repr(u8)]
pub enum AlertDescription {
    /// The sender will send no more data on this connection.
    CloseNotify = 0,
    /// A message arrived that the handshake state does not allow.
    UnexpectedMessage = 10,
    /// A record failed its integrity check.
    BadRecordMac = 20,
    /// Reserved; once reported a decryption failure.
    DecryptionFailedReserved = 21,
    /// A record was longer than the limit.
    RecordOverflow = 22,
    /// Decompression failed.
    DecompressionFailure = 30,
    /// No acceptable set of security parameters.
    HandshakeFailure = 40,
    /// Reserved; SSL 3.0 only.
    NoCertificateReserved = 41,
    /// A certificate was corrupt or its signature did not verify.
    BadCertificate = 42,
    /// A certificate was of an unsupported type.
    UnsupportedCertificate = 43,
    /// A certificate was revoked.
    CertificateRevoked = 44,
    /// A certificate has expired or is not yet valid.
    CertificateExpired = 45,
    /// Some other problem made the certificate unacceptable.
    CertificateUnknown = 46,
    /// A handshake field was out of range or inconsistent.
    IllegalParameter = 47,
    /// The chain led to no trusted CA.
    UnknownCa = 48,
    /// A valid certificate was refused by access control.
    AccessDenied = 49,
    /// A message could not be decoded.
    DecodeError = 50,
    /// A handshake cryptographic operation failed.
    DecryptError = 51,
    /// Reserved; export restrictions.
    ExportRestrictionReserved = 60,
    /// The offered protocol version is not supported.
    ProtocolVersion = 70,
    /// The server requires stronger ciphers than offered.
    InsufficientSecurity = 71,
    /// An error unrelated to the peer or the protocol.
    InternalError = 80,
    /// The handshake is being cancelled.
    UserCanceled = 90,
    /// A renegotiation request was refused.
    NoRenegotiation = 100,
    /// A hello carried an extension the sender did not ask for.
    UnsupportedExtension = 110,
    /// Any other description, preserved verbatim.
    #[catch_all]
    Other(u8),
}

/// One alert: its level and what it reports (RFC 5246 §7.2).
//~ models rfc5246#7.2
#[bin(big)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Alert {
    /// How serious the alert is.
    pub level: AlertLevel,
    /// What the alert reports.
    pub description: AlertDescription,
}

impl Alert {
    /// An alert with exactly these fields (no validation).
    #[must_use]
    pub const fn new(level: AlertLevel, description: AlertDescription) -> Self {
        Self { level, description }
    }
}
