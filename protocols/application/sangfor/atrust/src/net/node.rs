//! A connection to a tunnel node.

// --- Standard library ---
use std::io;
use std::net::TcpStream;
use std::time::Duration;

// --- Workspace dependencies ---
use crypto::RandomSource;
use tls::{ClientConfig, ServerVerification, blocking::Stream};

// --- Internal modules ---
use super::socket::{connect, server_name};
use crate::tcp::Transport;

/// Open a TLS connection to the node at `address`, `host:port` as a policy gives it
/// ([`Policy::node`](crate::policy::Policy::node)), for a tunnel to run over.
///
/// With explicit unverified TLS (spec §2.1), nothing else checks who answers:
/// whoever does receives the tunnel's signed request, session identifier included.
/// `random` must be a cryptographic source.
///
/// `timeout` bounds the connect and each later read or write, not the handshake as a whole.
/// The reference client gives a node 18 seconds to answer a tunnel's handshake and lifts the
/// limit once the tunnel is open (spec §9); here it stays on the socket, so a caller whose
/// stream may sit idle lifts it after opening the tunnel, with `set_read_timeout(None)` on
/// the socket ([`Stream::get_ref`]).
///
/// The returned stream borrows `address` for as long as it lives, because the TLS client's
/// configuration ties the server name to the connection's lifetime. Keep the address in a
/// string the stream can outlive.
///
/// # Errors
///
/// `InvalidInput` when `address` does not end in a colon and a port number, and any failure
/// to connect or to complete the TLS handshake.
pub fn dial<'a>(
    address: &'a str,
    timeout: Duration,
    verification: ServerVerification<'a>,
    random: &mut impl RandomSource,
) -> io::Result<Stream<'a, TcpStream>> {
    let (host, port) = split(address)?;
    let socket = connect(host, port, timeout)?;
    let config = ClientConfig {
        server_name: server_name(host),
        verification,
    };
    Ok(Stream::connect(socket, config, random)?)
}

/// The host and the port of a node address.
fn split(address: &str) -> io::Result<(&str, u16)> {
    let not_an_address =
        || io::Error::new(io::ErrorKind::InvalidInput, "a node address is host:port");
    let (host, port) = address.rsplit_once(':').ok_or_else(not_an_address)?;
    let port = port.parse().map_err(|_| not_an_address())?;
    if host.is_empty() {
        return Err(not_an_address());
    }
    Ok((host, port))
}

impl Transport for Stream<'_, TcpStream> {
    /// Sends TLS `close_notify`: this end writes nothing more, and reading goes on until the
    /// node closes too.
    fn close_write(&mut self) -> io::Result<()> {
        Ok(self.close()?)
    }
}

#[cfg(test)]
mod unit {
    use std::io::ErrorKind;

    use super::split;

    #[test]
    fn a_node_address_is_a_host_and_a_port() {
        assert_eq!(
            split("node.example.org:441").unwrap(),
            ("node.example.org", 441)
        );
        assert_eq!(split("192.0.2.1:4441").unwrap(), ("192.0.2.1", 4441));
        assert_eq!(split("[2001:db8::1]:441").unwrap(), ("[2001:db8::1]", 441));
        for address in [
            "node.example.org",
            "node.example.org:port",
            "node.example.org:70000",
            ":441",
            "",
        ] {
            assert_eq!(
                split(address).unwrap_err().kind(),
                ErrorKind::InvalidInput,
                "{address:?}"
            );
        }
    }
}
