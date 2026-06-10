use std::io::Read;
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use binrw::{io::Cursor, BinRead, BinWrite};

use crate::error::{Result, SocksError};
use crate::server::connection::{send_failure, send_reply};
use crate::v5::{Address, Request, Response, UdpHeader, UdpHeaderBuilder};

const MAX_DATAGRAM: usize = 65535;

/// Serves a UDP ASSOCIATE command (RFC 1928 section 7).
///
/// Binds a relay socket, replies with its address, and relays datagrams
/// until the TCP control connection closes.
pub(crate) fn handle_udp_associate(stream: TcpStream, request: &Request) -> Result<()> {
    let socket = match UdpSocket::bind((stream.local_addr()?.ip(), 0)) {
        Ok(socket) => socket,
        Err(err) => {
            send_failure(&stream, Response::GeneralFailure)?;
            return Err(err.into());
        }
    };

    send_reply(&stream, Response::Succeeded, socket.local_addr()?)?;

    // The request may advertise the client's UDP source; an unspecified
    // address means it is learned from the first datagram instead.
    let advertised = match request.dest_addr {
        Address::V4(addr) if !addr.is_unspecified() && request.dest_port != 0 => {
            Some(SocketAddr::from((addr, request.dest_port)))
        }
        Address::V6(addr) if !addr.is_unspecified() && request.dest_port != 0 => {
            Some(SocketAddr::from((addr, request.dest_port)))
        }
        _ => None,
    };

    let stop = Arc::new(AtomicBool::new(false));
    let relay_stop = Arc::clone(&stop);
    let relay = thread::spawn(move || relay_datagrams(socket, advertised, relay_stop));

    // Hold the association open until the control connection reaches EOF.
    let mut sink = [0u8; 128];
    while matches!((&stream).read(&mut sink), Ok(read) if read > 0) {}

    stop.store(true, Ordering::Relaxed);
    relay
        .join()
        .map_err(|_| SocksError::MessageParse("UDP relay thread panicked".to_string()))?
}

/// Relays datagrams between the client and remote hosts until stopped.
///
/// Client-originated datagrams are unwrapped and forwarded to their target;
/// datagrams from any other source are wrapped in a [`UdpHeader`] naming
/// that source and returned to the client.
fn relay_datagrams(
    socket: UdpSocket,
    advertised: Option<SocketAddr>,
    stop: Arc<AtomicBool>,
) -> Result<()> {
    socket.set_read_timeout(Some(Duration::from_secs(1)))?;

    let mut buf = vec![0u8; MAX_DATAGRAM];
    let mut client = advertised;

    loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }

        let (received, source) = match socket.recv_from(&mut buf) {
            Ok(received) => received,
            Err(err)
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(err) => return Err(err.into()),
        };

        let client_addr = *client.get_or_insert(source);

        if source == client_addr {
            // Outbound: unwrap the header and forward the payload. Malformed
            // or fragmented datagrams are dropped per RFC 1928.
            let mut cursor = Cursor::new(&buf[..received]);
            let Ok(header) = UdpHeader::read(&mut cursor) else {
                continue;
            };
            if header.frag != 0 {
                continue;
            }

            let start = cursor.position() as usize;
            let target = header.dest_addr.to_socket_string(header.dest_port);
            let _ = socket.send_to(&buf[start..received], target);
        } else {
            // Inbound: wrap the payload with the remote's address and return
            // it to the client.
            let address = Address::from(source);
            let header = UdpHeaderBuilder::default()
                .address_type(address.address_type())
                .dest_addr(address)
                .dest_port(source.port())
                .build()
                .map_err(|err| SocksError::MessageConstruction(err.to_string()))?;

            let mut cursor = Cursor::new(Vec::new());
            header.write(&mut cursor)?;

            let mut datagram = cursor.into_inner();
            datagram.extend_from_slice(&buf[..received]);
            let _ = socket.send_to(&datagram, client_addr);
        }
    }
}
