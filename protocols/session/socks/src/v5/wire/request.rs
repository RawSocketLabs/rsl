use super::{Command, DomainError, Endpoint, ReplyCode, VERSION};
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
use crate::error::Error;
use bnb::bin;

/// A client command request: `VER`, `CMD`, `RSV`, `ATYP`, destination, and port (RFC 1928 §4).
///
/// The builder validates the destination's domain length, not server command support.
/// Decoding remains permissive; raw fields and canonical/verbatim encoding are unchanged.
//~ models rfc1928#4 part="request"
#[bin(big, validate = Request::check_destination)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// The protocol version. The builder defaults to [`VERSION`].
    #[reserved_with(VERSION)]
    pub version: u8,
    /// The requested operation.
    pub command: Command,
    /// The reserved byte. The builder defaults to zero.
    #[reserved]
    pub reserved: u8,
    /// The destination endpoint, including its address-type byte.
    #[brw(variable)]
    pub destination: Endpoint,
}

impl Request {
    fn check_destination(&self) -> Result<(), DomainError> {
        self.destination.validate()
    }

    /// Check fixed header values without restricting the requested command.
    #[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
    pub(crate) fn check_header(&self) -> Result<(), Error> {
        super::validation::version(self.version, VERSION)?;
        super::validation::reserved(self.reserved)
    }
}

/// A server command reply: `VER`, `REP`, `RSV`, `ATYP`, bound address, and port (RFC 1928 §6).
///
/// The builder validates the bound domain's length without requiring a success code.
/// Decoding remains permissive, including replies with an empty domain.
//~ models rfc1928#6 part="reply"
#[bin(big, validate = Reply::check_bound)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    /// The protocol version. The builder defaults to [`VERSION`].
    #[reserved_with(VERSION)]
    pub version: u8,
    /// The request result.
    pub code: ReplyCode,
    /// The reserved byte. The builder defaults to zero.
    #[reserved]
    pub reserved: u8,
    /// The server-bound endpoint, including its address-type byte.
    #[brw(variable)]
    pub bound: Endpoint,
}

impl Reply {
    fn check_bound(&self) -> Result<(), DomainError> {
        self.bound.validate()
    }
}

#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
impl Reply {
    /// Require a compliant header, successful outcome, and usable bound-address length.
    pub(crate) fn ensure_success(&self) -> Result<(), Error> {
        super::validation::version(self.version, VERSION)?;
        super::validation::reserved(self.reserved)?;
        if self.code != ReplyCode::Succeeded {
            return Err(Error::Reply(self.code));
        }
        self.check_bound().map_err(|_| Error::InvalidEndpoint)
    }

    /// Construct a guided success reply after checking the supplied bound endpoint.
    pub(crate) fn success(bound: Endpoint) -> Result<Self, Error> {
        bound.validate().map_err(|_| Error::InvalidEndpoint)?;
        Ok(Self {
            version: VERSION,
            reserved: 0,
            code: ReplyCode::Succeeded,
            bound,
        })
    }

    /// Construct a failure with an unspecified IPv4 endpoint; reject success aliases.
    pub(crate) fn failure(code: ReplyCode) -> Result<Self, Error> {
        if u8::from(code) == 0 {
            return Err(Error::Reply(code));
        }
        Ok(Self {
            version: VERSION,
            reserved: 0,
            code,
            bound: std::net::SocketAddr::from(([0, 0, 0, 0], 0)).into(),
        })
    }
}
