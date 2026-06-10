use std::io::Write;
use std::net::TcpStream;

use binrw::{io::NoSeek, BinWrite};

use crate::error::{Result, SocksError};
use crate::v5::{
    Method, UserPassRequest, UserPassRequestBuilder, UserPassResponse, UserPassResponseBuilder,
    UserPassStatus,
};

/// Server-side authenticator for a single SOCKS5 method.
///
/// Runs the method-specific subnegotiation after the server has sent its
/// [`Offer`](crate::v5::Offer). Implementations exchange as many messages as
/// the method requires over the raw stream, which is how a future GSS-API
/// (RFC 1961) crate can plug in its multi-message token exchange.
pub trait Authenticator: Send + Sync {
    /// The method this authenticator negotiates.
    fn method(&self) -> Method;

    /// Performs the server side of the subnegotiation.
    ///
    /// # Errors
    /// Returns [`SocksError::AuthenticationFailed`] when the client's
    /// credentials are rejected, or an I/O or parse error if the exchange
    /// fails.
    fn authenticate(&self, stream: &mut TcpStream) -> Result<()>;
}

/// Client-side counterpart of [`Authenticator`].
pub trait AuthHandler {
    /// The method this handler negotiates.
    fn method(&self) -> Method;

    /// Performs the client side of the subnegotiation.
    ///
    /// # Errors
    /// Returns [`SocksError::AuthenticationFailed`] when the server rejects
    /// the credentials, or an I/O or parse error if the exchange fails.
    fn negotiate(&self, stream: &mut TcpStream) -> Result<()>;
}

/// The no-authentication method: the subnegotiation is empty.
pub struct NoAuth;

impl Authenticator for NoAuth {
    fn method(&self) -> Method {
        Method::NoAuth
    }

    fn authenticate(&self, _stream: &mut TcpStream) -> Result<()> {
        Ok(())
    }
}

impl AuthHandler for NoAuth {
    fn method(&self) -> Method {
        Method::NoAuth
    }

    fn negotiate(&self, _stream: &mut TcpStream) -> Result<()> {
        Ok(())
    }
}

/// Checks a username/password pair, returning `true` to accept it.
pub type Validator = Box<dyn Fn(&[u8], &[u8]) -> bool + Send + Sync>;

/// Server-side Username/Password authentication (RFC 1929).
///
/// Credentials are checked by a caller-supplied validator.
pub struct UserPassAuthenticator {
    validator: Validator,
}

impl UserPassAuthenticator {
    /// Creates an authenticator that accepts credentials for which
    /// `validator` returns `true`.
    pub fn new(validator: impl Fn(&[u8], &[u8]) -> bool + Send + Sync + 'static) -> Self {
        Self {
            validator: Box::new(validator),
        }
    }
}

impl Authenticator for UserPassAuthenticator {
    fn method(&self) -> Method {
        Method::Plain
    }

    fn authenticate(&self, stream: &mut TcpStream) -> Result<()> {
        let request = UserPassRequest::read_from(&mut &*stream)?;
        if request.version != 1 {
            return Err(SocksError::UnsupportedVersion(request.version));
        }

        let accepted = (self.validator)(&request.username, &request.password);
        let status = if accepted {
            UserPassStatus::Success
        } else {
            UserPassStatus::Custom(0x01)
        };

        let response = UserPassResponseBuilder::default()
            .status(status)
            .build()
            .map_err(|err| SocksError::MessageConstruction(err.to_string()))?;
        response.write(&mut NoSeek::new(&*stream))?;
        stream.flush()?;

        if accepted {
            Ok(())
        } else {
            Err(SocksError::AuthenticationFailed)
        }
    }
}

/// Client-side Username/Password authentication (RFC 1929).
pub struct UserPassHandler {
    username: Vec<u8>,
    password: Vec<u8>,
}

impl UserPassHandler {
    /// Creates a handler for the given credentials.
    ///
    /// # Errors
    /// Returns [`SocksError::Validation`] when either field exceeds the
    /// 255-byte wire limit.
    pub fn new(username: &str, password: &str) -> Result<Self> {
        if username.len() > 255 {
            return Err(SocksError::Validation(
                "username exceeds 255 bytes".to_string(),
            ));
        }
        if password.len() > 255 {
            return Err(SocksError::Validation(
                "password exceeds 255 bytes".to_string(),
            ));
        }

        Ok(Self {
            username: username.as_bytes().to_vec(),
            password: password.as_bytes().to_vec(),
        })
    }
}

impl AuthHandler for UserPassHandler {
    fn method(&self) -> Method {
        Method::Plain
    }

    fn negotiate(&self, stream: &mut TcpStream) -> Result<()> {
        let request = UserPassRequestBuilder::default()
            .username(self.username.clone())
            .password(self.password.clone())
            .build()
            .map_err(|err| SocksError::MessageConstruction(err.to_string()))?;
        request.write(&mut NoSeek::new(&*stream))?;

        let response = UserPassResponse::read_from(&mut &*stream)?;
        if response.status == UserPassStatus::Success {
            Ok(())
        } else {
            Err(SocksError::AuthenticationFailed)
        }
    }
}

#[cfg(test)]
mod unit {
    use std::net::TcpListener;

    use super::*;

    fn loopback_auth(
        authenticator: UserPassAuthenticator,
        handler: UserPassHandler,
    ) -> (Result<()>, Result<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            authenticator.authenticate(&mut stream)
        });

        let mut stream = TcpStream::connect(addr).unwrap();
        let client_result = handler.negotiate(&mut stream);

        (server.join().unwrap(), client_result)
    }

    #[test]
    fn test_user_pass_accepted() {
        let authenticator = UserPassAuthenticator::new(|u, p| u == b"user" && p == b"hunter2");
        let handler = UserPassHandler::new("user", "hunter2").unwrap();

        let (server_result, client_result) = loopback_auth(authenticator, handler);
        assert!(server_result.is_ok());
        assert!(client_result.is_ok());
    }

    #[test]
    fn test_user_pass_rejected() {
        let authenticator = UserPassAuthenticator::new(|u, p| u == b"user" && p == b"hunter2");
        let handler = UserPassHandler::new("user", "wrong").unwrap();

        let (server_result, client_result) = loopback_auth(authenticator, handler);
        assert!(matches!(
            server_result,
            Err(SocksError::AuthenticationFailed)
        ));
        assert!(matches!(
            client_result,
            Err(SocksError::AuthenticationFailed)
        ));
    }

    #[test]
    fn test_user_pass_handler_validates_length() {
        let long = "x".repeat(256);
        assert!(matches!(
            UserPassHandler::new(&long, "pass"),
            Err(SocksError::Validation(_))
        ));
    }
}
