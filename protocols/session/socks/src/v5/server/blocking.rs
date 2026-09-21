//! Embedded SOCKS5 exchange; callers own authorization, dialing, and deadlines.
use crate::io::blocking::{Deadline, deadline, write};
use crate::v5::{
    Endpoint, MethodRequest, MethodSelection, Reply, ReplyCode, Request as WireRequest,
    UsernamePasswordRequest, UsernamePasswordResponse, UsernamePasswordStatus, VERSION,
};
use crate::{
    Stream,
    error::Error,
    server::policy::ServerAuth,
    v5::{auth, decode},
};
use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};
/// An authenticated CONNECT request awaiting the caller's authorization and dial.
/// Dropping it closes an owned transport without acknowledging success.
pub struct Request<S> {
    pub(crate) stream: Stream<S>,
    pub(crate) destination: Endpoint,
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
        write(&mut self.stream, &Reply::success(bound)?)?;
        Ok(self.stream)
    }

    /// Send a failing reply and drop the transport. A success code is rejected.
    ///
    /// # Errors
    /// Returns an I/O/codec error or rejects an attempted success reply.
    pub fn send_failure(mut self, code: ReplyCode) -> Result<(), Error> {
        write(&mut self.stream, &Reply::failure(code)?)
    }
}

/// Authenticate and read one CONNECT request; does not dial or authorize a target.
/// Generic transports need caller-enforced deadlines. No credential data is logged.
///
/// # Errors
/// Failures are terminal. Unsupported commands/address types receive their SOCKS
/// failure reply; malformed greetings close without continuing negotiation.
pub fn exchange<S: Read + Write>(stream: S, auth: &ServerAuth) -> Result<Request<S>, Error> {
    let mut stream = Stream::new(stream);
    let offer: MethodRequest = stream.read_message()?;
    if offer.version != VERSION {
        return Err(Error::VersionNotAccepted(offer.version));
    }
    let selected = auth::select(auth, &offer);
    write(
        &mut stream,
        &MethodSelection {
            version: VERSION,
            method: selected
                .as_ref()
                .copied()
                .unwrap_or(crate::v5::AuthMethod::NoAcceptable),
        },
    )?;
    selected?;
    if auth::server_method(auth) == crate::v5::AuthMethod::UsernamePassword {
        let credentials: UsernamePasswordRequest = stream.read_message()?;
        let accepted = auth::authenticate(auth, &credentials);
        write(
            &mut stream,
            &UsernamePasswordResponse {
                version: 1,
                status: UsernamePasswordStatus::from(u8::from(!accepted)),
            },
        )?;
        if !accepted {
            return Err(Error::AuthenticationRejected);
        }
    }
    let result = stream
        .read_message::<WireRequest>()
        .map_err(|error| decode::command_error(error, &mut stream.buffered))
        .and_then(|request| {
            super::validation::check_request(&request)?;
            Ok(request)
        });
    match result {
        Ok(request) => Ok(Request {
            stream,
            destination: request.destination,
            authentication: if auth.requires_credentials() {
                crate::server::policy::Authentication::UsernamePassword
            } else {
                crate::server::policy::Authentication::Unauthenticated
            },
        }),
        Err(error) => {
            write(&mut stream, &Reply::failure(error.reply_code())?)?;
            Err(error)
        }
    }
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
