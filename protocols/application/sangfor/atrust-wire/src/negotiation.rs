//! The opening message of a tunnel connection.

// --- Workspace dependencies ---
use bnb::bin;

/// What a client sends first on a tunnel connection: a SOCKS5-style greeting that offers one
/// private method, run together with that method's sub-negotiation, which carries a JSON
/// document (spec §7.1 and §9.1).
///
/// The same layout opens both kinds of connection; the method and status bytes say which.
/// Dual-use: every byte is stored, so any greeting can be sent or decoded. More than 255
/// methods do not encode.
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Negotiation {
    /// The protocol version, 5.
    pub version: u8,
    /// The methods offered: one, in every opening a client sends. Their count goes in front
    /// as one byte.
    #[brw(count_prefix = u8)]
    pub methods: Vec<u8>,
    /// The sub-negotiation marker, `'S'`.
    pub marker: u8,
    /// The sub-negotiation status byte.
    pub status: u8,
    /// The JSON document, uninterpreted. Its length goes in front as 16 bits.
    #[brw(count_prefix = u16)]
    pub json: Vec<u8>,
}

impl Negotiation {
    /// The protocol version byte.
    pub const VERSION: u8 = 0x05;
    /// The sub-negotiation marker, the letter `S`.
    pub const MARKER: u8 = 0x53;
    /// The method of a session-authentication connection (VIP fetch and L3 tunnel).
    pub const METHOD_SESSION: u8 = 0xd0;
    /// The method of a TCP tunnel connection.
    pub const METHOD_TCP: u8 = 0x81;
    /// The status byte a client sends when authenticating a session.
    pub const STATUS_SESSION: u8 = 0x00;
    /// The status byte a client sends when opening a TCP tunnel.
    pub const STATUS_TCP: u8 = 0x03;

    /// The opening of a session-authentication connection, carrying `json`.
    #[must_use]
    pub fn session(json: Vec<u8>) -> Self {
        Self::new(Self::METHOD_SESSION, Self::STATUS_SESSION, json)
    }

    /// The opening of a TCP tunnel connection, carrying the signed `json`.
    #[must_use]
    pub fn tcp(json: Vec<u8>) -> Self {
        Self::new(Self::METHOD_TCP, Self::STATUS_TCP, json)
    }

    /// An opening that offers this one method, with the usual version and marker.
    fn new(method: u8, status: u8, json: Vec<u8>) -> Self {
        Self {
            version: Self::VERSION,
            methods: vec![method],
            marker: Self::MARKER,
            status,
            json,
        }
    }
}
