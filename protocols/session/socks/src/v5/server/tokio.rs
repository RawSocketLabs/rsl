//! Embedded Tokio SOCKS5 exchange; policy, dialing, and deadlines remain caller-owned.
use crate::io::tokio::write;
use crate::v5::{
    AuthMethod, Endpoint, MethodRequest, MethodSelection, Reply, ReplyCode, Request as WireRequest,
    UsernamePasswordRequest, UsernamePasswordResponse, UsernamePasswordStatus, VERSION,
};
use crate::{Stream, error::Error, server::policy::ServerAuth, v5::auth};
use tokio::io::{AsyncRead, AsyncWrite};
/// An authenticated CONNECT request awaiting authorization and a target connection.
pub struct Request<S> {
    stream: Stream<S>,
    destination: Endpoint,
    authentication: crate::server::policy::Authentication,
}

impl<S: AsyncRead + AsyncWrite + Unpin> Request<S> {
    /// Authentication completed by this exchange; contains no credentials.
    pub fn authentication(&self) -> crate::server::policy::Authentication {
        self.authentication
    }

    /// The original destination, including unresolved domain bytes.
    pub fn destination(&self) -> &Endpoint {
        &self.destination
    }

    /// Acknowledge an established target connection, then return a lossless tunnel.
    /// `bound` must be the target connection's local endpoint.
    ///
    /// # Errors
    /// Returns a terminal codec or transport error. Cancellation is also terminal.
    pub async fn send_success(mut self, bound: Endpoint) -> Result<Stream<S>, Error> {
        write(&mut self.stream, &Reply::success(bound)?).await?;
        Ok(self.stream)
    }

    /// Send a failing reply and drop the transport. Success codes are rejected.
    ///
    /// # Errors
    /// Returns an I/O/codec error or rejects an attempted success reply.
    pub async fn send_failure(mut self, code: ReplyCode) -> Result<(), Error> {
        write(&mut self.stream, &Reply::failure(code)?).await
    }
}

/// Authenticate and read a CONNECT request without dialing or authorizing it.
/// Callers manage deadlines; credential callbacks must not block the runtime.
///
/// # Errors
/// Returns terminal handshake errors; unsupported commands/address types receive
/// a SOCKS failure reply. Cancelling the future requires discarding the transport.
pub async fn exchange<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    auth: &ServerAuth,
) -> Result<Request<S>, Error> {
    let mut stream = Stream::new(stream);
    let offer: MethodRequest = stream.read_message_async().await?;
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
                .unwrap_or(AuthMethod::NoAcceptable),
        },
    )
    .await?;
    selected?;
    if auth::server_method(auth) == AuthMethod::UsernamePassword {
        let credentials: UsernamePasswordRequest = stream.read_message_async().await?;
        let accepted = auth::authenticate(auth, &credentials);
        write(
            &mut stream,
            &UsernamePasswordResponse {
                version: 1,
                status: UsernamePasswordStatus::from(u8::from(!accepted)),
            },
        )
        .await?;
        if !accepted {
            return Err(Error::AuthenticationRejected);
        }
    }
    let result = stream
        .read_message_async::<WireRequest>()
        .await
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
            write(&mut stream, &Reply::failure(error.reply_code())?).await?;
            Err(error)
        }
    }
}
