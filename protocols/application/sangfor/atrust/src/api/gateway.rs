//! Where the gateway is.

/// A gateway's control-plane address (spec §4).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Gateway {
    /// The host: a name or an address literal.
    host: String,
    /// The HTTPS port.
    port: u16,
}

impl Gateway {
    /// The port HTTPS uses when none is given, which is left out of the base host.
    pub const DEFAULT_PORT: u16 = 443;

    /// The gateway at `host`, listening for HTTPS on `port`. `host` is a name or an address
    /// without a port; an IPv6 address is written in brackets, as in a URL. It is not
    /// checked here: a host that cannot appear in a request fails when one is built.
    #[must_use]
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }

    /// The host, without a port. A policy's node addresses may stand for it (spec §6.4).
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The HTTPS port.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// The "base host": the host alone on the default port, else `host:port`. It is the
    /// `Host` header, the input of the RID header, and the scope of the session's cookies.
    #[must_use]
    pub fn base_host(&self) -> String {
        if self.port == Self::DEFAULT_PORT {
            return self.host.clone();
        }
        format!("{}:{}", self.host, self.port)
    }
}
