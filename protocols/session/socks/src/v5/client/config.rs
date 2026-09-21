use crate::{
    error::Error,
    v5::{UsernamePasswordRequest, auth::ClientAuth},
};

/// Owned SOCKS5 client authentication configuration, reusable across connections.
/// Offers exactly one method. Credentials have no `Debug` and are not zeroized.
pub struct Config {
    credentials: Option<UsernamePasswordRequest>,
}

impl Config {
    /// Explicitly use an unauthenticated proxy connection.
    #[must_use]
    pub const fn no_authentication() -> Self {
        Self { credentials: None }
    }

    /// Require RFC 1929 authentication; never fall back to no-auth.
    ///
    /// # Errors
    /// Both credential lengths must be between one and 255 bytes inclusive.
    pub fn username_password(
        username: impl Into<Vec<u8>>,
        password: impl Into<Vec<u8>>,
    ) -> Result<Self, Error> {
        Ok(Self {
            credentials: Some(
                UsernamePasswordRequest::builder()
                    .username(username.into())
                    .password(password.into())
                    .build()?,
            ),
        })
    }

    /// Borrow authentication for the version-specific drivers without cloning configuration.
    #[must_use]
    pub fn authentication(&self) -> ClientAuth<'_> {
        match &self.credentials {
            None => ClientAuth::NoAuthentication,
            Some(credentials) => ClientAuth::UsernamePassword {
                username: &credentials.username,
                password: &credentials.password,
            },
        }
    }
}
