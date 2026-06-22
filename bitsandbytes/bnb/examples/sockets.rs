//! **sockets** — the ergonomic `std` socket helpers (the `net` feature): [`MessageStream`]
//! over TCP and [`MessageDatagram`] over UDP *and* Unix-domain datagram sockets.
//!
//! The convenience counterpart to the raw `tcp`/`dns` examples. `MessageStream::new(stream)`
//! *owns* a `TcpStream` and exposes `read_message`/`write_message` — both directions on one
//! connection, no `try_clone`, no `&TcpStream` borrow dance. `MessageDatagram::new(socket)` does
//! the same for any `DatagramSocket` (here a `UdpSocket` and a `UnixDatagram` — the *same* code).
//!
//! Run with: `cargo run -p bitsandbytes --example sockets --features net`

use bnb::{MessageDatagram, MessageStream, bin};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::thread;
use std::time::Duration;
use tracing::info;

#[bin(big, magic = b"RPC")]
#[derive(Debug, Clone, PartialEq, Eq)]
enum Message {
    #[bin(magic = 0x01u8)]
    Ping { seq: u32 },
    #[bin(magic = 0x02u8)]
    Pong { seq: u32 },
    #[bin(magic = 0x03u8)]
    Bye,
}

/// TCP: a server thread and a client, each wrapping its `TcpStream` in a `MessageStream`.
fn tcp_demo() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept");
        let mut conn = MessageStream::new(stream); // owns the socket; both read + write
        while let Ok(req) = conn.read_message::<Message>() {
            info!(?req, "tcp server ← request");
            match req {
                Message::Ping { seq } => conn.write_message(&Message::Pong { seq }).expect("write"),
                Message::Bye => break,
                Message::Pong { .. } => {}
            }
        }
    });

    let mut conn = MessageStream::new(TcpStream::connect(addr)?);
    for seq in [1u32, 2] {
        conn.write_message(&Message::Ping { seq })?;
        let reply = conn.read_message::<Message>()?;
        info!(?reply, "tcp client ← reply");
        assert_eq!(reply, Message::Pong { seq });
    }
    conn.write_message(&Message::Bye)?;
    server.join().expect("server thread");
    Ok(())
}

/// UDP: a `MessageDatagram` per socket (sequential — the datagram is buffered by the OS between
/// send and recv).
fn udp_demo() -> Result<(), Box<dyn std::error::Error>> {
    let server_sock = UdpSocket::bind("127.0.0.1:0")?;
    server_sock.set_read_timeout(Some(Duration::from_secs(2)))?;
    let server_addr = server_sock.local_addr()?;
    let mut server = MessageDatagram::new(server_sock);

    let client_sock = UdpSocket::bind("127.0.0.1:0")?;
    client_sock.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut client = MessageDatagram::new(client_sock);

    client.send_message(&Message::Ping { seq: 9 }, &server_addr)?;
    let (req, from) = server.recv_message::<Message>()?;
    info!(?req, "udp server ← request");

    server.send_message(&Message::Pong { seq: 9 }, &from)?;
    let (reply, _) = client.recv_message::<Message>()?;
    info!(?reply, "udp client ← reply");
    assert_eq!(reply, Message::Pong { seq: 9 });
    Ok(())
}

/// Unix-domain datagram: the *exact same* `MessageDatagram` API over a different transport.
#[cfg(unix)]
fn unix_demo() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::net::UnixDatagram;
    let dir = std::env::temp_dir();
    let spath = dir.join(format!("bnb-sockets-srv-{}.sock", std::process::id()));
    let cpath = dir.join(format!("bnb-sockets-cli-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&spath);
    let _ = std::fs::remove_file(&cpath);

    let server_sock = UnixDatagram::bind(&spath)?;
    let server_addr = server_sock.local_addr()?;
    let mut server = MessageDatagram::new(server_sock);
    let client = MessageDatagram::new(UnixDatagram::bind(&cpath)?);

    client.send_message(&Message::Ping { seq: 42 }, &server_addr)?;
    let (req, _from) = server.recv_message::<Message>()?;
    info!(?req, "unix server ← request");
    assert_eq!(req, Message::Ping { seq: 42 });

    let _ = std::fs::remove_file(&spath);
    let _ = std::fs::remove_file(&cpath);
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .without_time()
        .init();

    info!("--- TCP (MessageStream) ---");
    tcp_demo()?;
    info!("--- UDP (MessageDatagram) ---");
    udp_demo()?;
    #[cfg(unix)]
    {
        info!("--- Unix datagram (MessageDatagram, same API) ---");
        unix_demo()?;
    }
    info!("all checks passed");
    Ok(())
}
