//! Blocking server authentication and CONNECT validation.
// --- Standard library ---
use std::io::{Read, Write};

// --- Internal modules ---
use crate::v5::{
    Endpoint, MethodRequest, MethodSelection, Reply, Request as WireRequest,
    UsernamePasswordRequest, UsernamePasswordResponse, UsernamePasswordStatus, VERSION,
};
use crate::{
    Stream,
    error::Error,
    server::policy::{Authentication, ServerAuth},
    v5::auth,
};

/// Facts and retained input established before the CONNECT reply.
pub(super) struct AuthenticatedRequest<S> {
    pub(super) stream: Stream<S>,
    pub(super) destination: Endpoint,
    pub(super) authentication: Authentication,
}

/// Authenticate and accept only a valid CONNECT request.
pub(super) fn exchange<S: Read + Write>(
    stream: S,
    auth: &ServerAuth,
) -> Result<AuthenticatedRequest<S>, Error> {
    let mut stream = Stream::new(stream);
    let offer: MethodRequest = stream.read_message()?;
    if offer.version != VERSION {
        return Err(Error::VersionNotAccepted(offer.version));
    }
    let selected = auth::select(auth, &offer);
    stream.write_message(&MethodSelection {
        version: VERSION,
        method: selected
            .as_ref()
            .copied()
            .unwrap_or(crate::v5::AuthMethod::NoAcceptable),
    })?;
    selected?;
    if auth::server_method(auth) == crate::v5::AuthMethod::UsernamePassword {
        let credentials: UsernamePasswordRequest = stream.read_message()?;
        let accepted = auth::authenticate(auth, &credentials);
        stream.write_message(&UsernamePasswordResponse {
            version: 1,
            status: UsernamePasswordStatus::from(u8::from(!accepted)),
        })?;
        if !accepted {
            return Err(Error::AuthenticationRejected);
        }
    }
    let result = stream.read_message::<WireRequest>().and_then(|request| {
        crate::v5::server::validation::check_request(&request)?;
        Ok(request)
    });
    match result {
        Ok(request) => Ok(AuthenticatedRequest {
            stream,
            destination: request.destination,
            authentication: if auth.requires_credentials() {
                crate::server::policy::Authentication::UsernamePassword
            } else {
                crate::server::policy::Authentication::Unauthenticated
            },
        }),
        Err(error) => {
            stream.write_message(&Reply::failure(error.reply_code())?)?;
            Err(error)
        }
    }
}
