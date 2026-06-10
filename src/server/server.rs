use std::net::{SocketAddr, TcpListener, ToSocketAddrs};
use std::sync::Arc;
use std::thread;

use crate::auth::{Authenticator, NoAuth};
use crate::error::Result;
use crate::server::connection::handle_client;

/// A SOCKS5 proxy server.
pub struct Server {
    listener: TcpListener,
    authenticators: Arc<Vec<Box<dyn Authenticator>>>,
}

impl Server {
    /// Binds a server that accepts unauthenticated clients.
    ///
    /// # Errors
    /// Returns an error if the listener cannot be bound.
    pub fn bind(addr: impl ToSocketAddrs) -> Result<Self> {
        Ok(Self {
            listener: TcpListener::bind(addr)?,
            authenticators: Arc::new(vec![Box::new(NoAuth)]),
        })
    }

    /// Replaces the accepted authentication methods, in preference order.
    pub fn with_authenticators(mut self, authenticators: Vec<Box<dyn Authenticator>>) -> Self {
        self.authenticators = Arc::new(authenticators);
        self
    }

    /// The address the server is listening on.
    ///
    /// # Errors
    /// Returns an error if the socket address cannot be retrieved.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.listener.local_addr()?)
    }

    /// Accepts and serves a single client on the current thread.
    ///
    /// # Errors
    /// Returns an error if accepting fails or the client's session ends in
    /// an error.
    pub fn accept(&self) -> Result<()> {
        let (stream, _) = self.listener.accept()?;
        handle_client(stream, &self.authenticators)
    }

    /// Serves clients until the listener fails, one thread per connection.
    ///
    /// # Errors
    /// Returns an error if accepting a connection fails.
    pub fn serve(&self) -> Result<()> {
        loop {
            let (stream, _) = self.listener.accept()?;
            let authenticators = Arc::clone(&self.authenticators);
            thread::spawn(move || {
                let _ = handle_client(stream, &authenticators);
            });
        }
    }
}
