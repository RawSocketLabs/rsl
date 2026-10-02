//! SOCKS5 client authentication choices and server method adaptation.
// --- Internal modules ---
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
use super::{
    AuthMethod, MethodRequest, USERNAME_PASSWORD_VERSION, UsernamePasswordRequest, VERSION,
};
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
use crate::{error::Error, server::policy::ServerAuth};

/// Exactly one method is offered, preventing an implicit downgrade to no-auth.
/// Credentials are borrowed and deliberately have no `Debug` implementation.
#[derive(Clone, Copy)]
pub enum ClientAuth<'a> {
    /// Explicitly permit an unauthenticated proxy connection.
    NoAuthentication,
    /// Require RFC 1929 username/password authentication.
    UsernamePassword {
        /// Username bytes (1–255 bytes).
        user: &'a [u8],
        /// Password bytes (1–255 bytes).
        pass: &'a [u8],
    },
}

#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
impl ClientAuth<'_> {
    pub(crate) fn method(self) -> AuthMethod {
        match self {
            Self::NoAuthentication => AuthMethod::NoAuthentication,
            Self::UsernamePassword { .. } => AuthMethod::UsernamePassword,
        }
    }

    pub(crate) fn credentials(self) -> Result<Option<UsernamePasswordRequest>, Error> {
        match self {
            Self::NoAuthentication => Ok(None),
            Self::UsernamePassword { user, pass } => Ok(Some(
                UsernamePasswordRequest::builder()
                    .username(user.to_vec())
                    .password(pass.to_vec())
                    .build()?,
            )),
        }
    }
}

#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
pub(crate) fn server_method(auth: &ServerAuth) -> AuthMethod {
    if auth.requires_credentials() {
        AuthMethod::UsernamePassword
    } else {
        AuthMethod::NoAuthentication
    }
}
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
pub(crate) fn select(auth: &ServerAuth, offer: &MethodRequest) -> Result<AuthMethod, Error> {
    if offer.version != VERSION {
        return Err(Error::VersionNotAccepted(offer.version));
    }
    if offer.is_valid() && offer.methods.contains(&server_method(auth)) {
        Ok(server_method(auth))
    } else {
        Err(Error::NoAcceptableMethod)
    }
}
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
pub(crate) fn authenticate(auth: &ServerAuth, request: &UsernamePasswordRequest) -> bool {
    request.version == USERNAME_PASSWORD_VERSION
        && request.is_valid()
        && auth.verify(&request.username, &request.password)
}
