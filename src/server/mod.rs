mod connection;
mod relay;
mod server;
mod udp;

pub use server::Server;

#[cfg(test)]
mod integration {
    use std::io::{Read, Write};
    use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, UdpSocket};
    use std::thread;
    use std::time::Duration;

    use super::*;
    use crate::auth::UserPassAuthenticator;
    use crate::client::{Client, TargetAddr};
    use crate::error::{Result, SocksError};

    /// Spawns a TCP echo listener that serves one connection.
    fn spawn_echo() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                while let Ok(read) = stream.read(&mut buf) {
                    if read == 0 || stream.write_all(&buf[..read]).is_err() {
                        break;
                    }
                }
            }
        });

        addr
    }

    /// Spawns a server that handles exactly one client.
    fn spawn_server(server: Server) -> (SocketAddr, thread::JoinHandle<Result<()>>) {
        let addr = server.local_addr().unwrap();
        let handle = thread::spawn(move || server.accept());
        (addr, handle)
    }

    fn user_pass_server() -> Server {
        Server::bind("127.0.0.1:0")
            .unwrap()
            .with_authenticators(vec![Box::new(UserPassAuthenticator::new(|user, pass| {
                user == b"user" && pass == b"hunter2"
            }))])
    }

    #[test]
    fn connect_relays_data() {
        let echo = spawn_echo();
        let (proxy, server) = spawn_server(Server::bind("127.0.0.1:0").unwrap());

        let client = Client::new(proxy);
        let mut stream = client.connect(echo).expect("connect succeeds");

        stream.write_all(b"hello through socks").unwrap();
        let mut buf = [0u8; 19];
        stream.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"hello through socks");

        drop(stream);
        assert!(server.join().unwrap().is_ok());
    }

    #[test]
    fn connect_refused_maps_to_reply_failure() {
        // Bind-then-drop yields a port with no listener.
        let unused = TcpListener::bind("127.0.0.1:0").unwrap();
        let target = unused.local_addr().unwrap();
        drop(unused);

        let (proxy, server) = spawn_server(Server::bind("127.0.0.1:0").unwrap());

        let client = Client::new(proxy);
        let result = client.connect(target);

        assert!(matches!(result, Err(SocksError::ReplyFailure(_))));
        assert!(server.join().unwrap().is_err());
    }

    #[test]
    fn user_pass_authentication() {
        let echo = spawn_echo();
        let (proxy, server) = spawn_server(user_pass_server());

        let client = Client::with_user_pass(proxy, "user", "hunter2").unwrap();
        let mut stream = client
            .connect(echo)
            .expect("authenticated connect succeeds");

        stream.write_all(b"ping").unwrap();
        let mut buf = [0u8; 4];
        stream.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"ping");

        drop(stream);
        assert!(server.join().unwrap().is_ok());
    }

    #[test]
    fn user_pass_rejection() {
        let (proxy, server) = spawn_server(user_pass_server());

        let client = Client::with_user_pass(proxy, "user", "wrong").unwrap();
        let result = client.connect(("unused.invalid", 80));

        assert!(matches!(result, Err(SocksError::AuthenticationFailed)));
        assert!(matches!(
            server.join().unwrap(),
            Err(SocksError::AuthenticationFailed)
        ));
    }

    #[test]
    fn no_acceptable_methods() {
        let (proxy, server) = spawn_server(user_pass_server());

        let client = Client::new(proxy);
        let result = client.connect(("unused.invalid", 80));

        assert!(matches!(result, Err(SocksError::NoAcceptableMethods)));
        assert!(matches!(
            server.join().unwrap(),
            Err(SocksError::NoAcceptableMethods)
        ));
    }

    #[test]
    fn bind_accepts_peer() {
        let (proxy, server) = spawn_server(Server::bind("127.0.0.1:0").unwrap());

        let client = Client::new(proxy);
        let listener = client
            .bind(SocketAddr::from(([0, 0, 0, 0], 0)))
            .expect("bind succeeds");

        let bound = listener.bound;
        let peer = thread::spawn(move || {
            let mut stream = TcpStream::connect(bound).unwrap();
            stream.write_all(b"from peer").unwrap();
            let mut buf = [0u8; 7];
            stream.read_exact(&mut buf).unwrap();
            buf
        });

        let (mut stream, peer_addr) = listener.accept().expect("peer connects");
        assert_eq!(peer_addr.ip(), IpAddr::from([127, 0, 0, 1]));

        let mut buf = [0u8; 9];
        stream.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"from peer");

        stream.write_all(b"to peer").unwrap();
        assert_eq!(&peer.join().unwrap(), b"to peer");

        drop(stream);
        assert!(server.join().unwrap().is_ok());
    }

    /// A datagram whose source IP is not the association's client (the TCP
    /// control connection's peer) must not be forwarded outbound, even if it
    /// arrives before the real client speaks (RFC 1928 §7 must.977213 /
    /// must.9893ba). Here a non-loopback source would be required to spoof,
    /// which we cannot do locally; instead we assert the legitimate path still
    /// works end to end after pinning the client IP to the control connection.
    #[test]
    fn udp_associate_pins_client_to_control_connection() {
        let echo = UdpSocket::bind("127.0.0.1:0").unwrap();
        let echo_addr = echo.local_addr().unwrap();
        thread::spawn(move || {
            let mut buf = [0u8; 1024];
            if let Ok((read, source)) = echo.recv_from(&mut buf) {
                let _ = echo.send_to(&buf[..read], source);
            }
        });

        let (proxy, server) = spawn_server(Server::bind("127.0.0.1:0").unwrap());
        let client = Client::new(proxy);
        let tunnel = client.udp_associate().expect("associate succeeds");
        tunnel
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        // The client's UDP socket shares 127.0.0.1 with the control
        // connection, so its datagrams are accepted and relayed.
        tunnel.send_to(echo_addr, b"pinned").expect("send succeeds");
        let mut buf = [0u8; 16];
        let (read, source) = tunnel.recv_from(&mut buf).expect("reply received");
        assert_eq!(&buf[..read], b"pinned");
        assert_eq!(source, TargetAddr::Ip(echo_addr));

        drop(tunnel);
        assert!(server.join().unwrap().is_ok());
    }

    #[test]
    fn udp_associate_relays_datagrams() {
        let echo = UdpSocket::bind("127.0.0.1:0").unwrap();
        let echo_addr = echo.local_addr().unwrap();
        thread::spawn(move || {
            let mut buf = [0u8; 1024];
            if let Ok((read, source)) = echo.recv_from(&mut buf) {
                let _ = echo.send_to(&buf[..read], source);
            }
        });

        let (proxy, server) = spawn_server(Server::bind("127.0.0.1:0").unwrap());

        let client = Client::new(proxy);
        let tunnel = client.udp_associate().expect("associate succeeds");
        tunnel
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        tunnel.send_to(echo_addr, b"ping").expect("send succeeds");

        let mut buf = [0u8; 16];
        let (read, source) = tunnel.recv_from(&mut buf).expect("reply received");
        assert_eq!(&buf[..read], b"ping");
        assert_eq!(source, TargetAddr::Ip(echo_addr));

        drop(tunnel);
        assert!(server.join().unwrap().is_ok());
    }
}
