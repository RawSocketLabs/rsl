//! TCP connections with a time limit.

// --- Standard library ---
use std::io;
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Connect to `host:port`, trying each address the name resolves to, and bound every later
/// read and write by `timeout`. A read or write that runs out of time fails with `WouldBlock`
/// or `TimedOut`, as the platform has it.
///
/// shortcut: `timeout` bounds the connect and each read or write, not the whole exchange,
/// and name resolution is not bounded at all; upgrade to a deadline when one is needed.
pub(super) fn connect(host: &str, port: u16, timeout: Duration) -> io::Result<TcpStream> {
    let mut last = io::Error::new(io::ErrorKind::NotFound, "the host has no address");
    for address in (bare(host), port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(socket) => {
                socket.set_read_timeout(Some(timeout))?;
                socket.set_write_timeout(Some(timeout))?;
                socket.set_nodelay(true)?;
                return Ok(socket);
            }
            Err(error) => last = error,
        }
    }
    Err(last)
}

/// The name to offer in the TLS handshake: the host without a trailing dot, unless it is an
/// address literal, for which none is sent (spec §2.1).
pub(super) fn server_name(host: &str) -> Option<&str> {
    let name = host.strip_suffix('.').unwrap_or(host);
    bare(host).parse::<IpAddr>().is_err().then_some(name)
}

/// A host without the brackets an IPv6 literal is written in.
fn bare(host: &str) -> &str {
    host.strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host)
}

#[cfg(test)]
mod unit {
    use super::server_name;

    // Spec §2.1: a name is offered, an address literal is not.
    #[test]
    fn a_server_name_is_offered_for_names_only() {
        assert_eq!(server_name("sdp.example.org"), Some("sdp.example.org"));
        assert_eq!(server_name("sdp.example.org."), Some("sdp.example.org"));
        for literal in ["192.0.2.1", "2001:db8::1", "[2001:db8::1]", "::1", "[::1]"] {
            assert_eq!(server_name(literal), None, "{literal}");
        }
    }
}
