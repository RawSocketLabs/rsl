//! Embedded SOCKS5 exchange; callers own authorization, dialing, and deadlines.
// --- Standard library ---
use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

// --- Internal modules ---
use crate::io::blocking::{Deadline, deadline};
use crate::v5::{Endpoint, Reply, ReplyCode};
use crate::{Stream, error::Error, server::policy::ServerAuth};
/// An authenticated CONNECT request awaiting the caller's authorization and dial.
/// Dropping it closes an owned transport without acknowledging success.
pub struct Request<S> {
    pub(super) stream: Stream<S>,
    pub(super) destination: Endpoint,
    authentication: crate::server::policy::Authentication,
}

impl<S: Read + Write> Request<S> {
    /// Authentication completed by this exchange; contains no credentials.
    pub fn authentication(&self) -> crate::server::policy::Authentication {
        self.authentication
    }

    /// The original destination, including unresolved domain bytes.
    pub fn destination(&self) -> &Endpoint {
        &self.destination
    }

    /// Send success only after connecting to the target, using its local address
    /// as `bound`, then return the transport with all tunnel bytes preserved.
    ///
    /// # Errors
    /// Returns a codec or transport error; the session must not be reused.
    pub fn send_success(mut self, bound: Endpoint) -> Result<Stream<S>, Error> {
        self.stream.write_message(&Reply::success(bound)?)?;
        Ok(self.stream)
    }

    /// Send a failing reply and drop the transport. A success code is rejected.
    ///
    /// # Errors
    /// Returns an I/O/codec error or rejects an attempted success reply.
    pub fn send_failure(mut self, code: ReplyCode) -> Result<(), Error> {
        self.stream.write_message(&Reply::failure(code)?)
    }
}

/// Authenticate and read one CONNECT request; callers own authorization and deadlines.
///
/// # Errors
/// Failures are terminal; unsupported commands receive a failing SOCKS reply.
pub fn exchange<S: Read + Write>(stream: S, auth: &ServerAuth) -> Result<Request<S>, Error> {
    let request = super::exchange::exchange(stream, auth, crate::v5::Command::Connect)?;
    Ok(Request {
        stream: request.stream,
        destination: request.destination,
        authentication: request.authentication,
    })
}

impl Request<TcpStream> {
    pub(crate) fn bounded(self, timeout: Duration) -> Result<Request<Deadline>, Error> {
        let (stream, buffered) = self.stream.into_parts();
        Ok(Request {
            stream: Stream::from_parts(
                Deadline {
                    stream,
                    deadline: deadline(timeout)?,
                },
                buffered,
            ),
            destination: self.destination,
            authentication: self.authentication,
        })
    }
}

impl Request<Deadline> {
    pub(crate) fn without_deadline(self) -> Result<Request<TcpStream>, Error> {
        let (stream, buffered) = self.stream.into_parts();
        stream.stream.set_read_timeout(None)?;
        stream.stream.set_write_timeout(None)?;
        Ok(Request {
            stream: Stream::from_parts(stream.stream, buffered),
            destination: self.destination,
            authentication: self.authentication,
        })
    }
}
