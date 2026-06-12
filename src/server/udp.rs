use std::collections::HashSet;
use std::io::Read;
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use binrw::{BinRead, BinWrite, io::Cursor};

use crate::error::{Result, SocksError};
use crate::server::connection::{send_failure, send_reply};
use crate::v5::{Address, Request, Response, UdpHeader, UdpHeaderBuilder};

const MAX_DATAGRAM: usize = 65535;

/// Cap on distinct remote destinations tracked per association, so a long-lived
/// client that talks to many hosts cannot grow the relay's memory without
/// bound. Beyond this, new destinations are still forwarded outbound but their
/// replies are not relayed back (a degradation only an abnormal association
/// reaches).
const MAX_UDP_TARGETS: usize = 1024;

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

    // BND.ADDR/BND.PORT in the reply are the relay socket the client sends to.
    //~ implements rfc1928#6/must.31530e
    send_reply(&stream, Response::Succeeded, socket.local_addr()?)?;
    tracing::debug!(relay = ?socket.local_addr().ok(), "UDP associate established");

    // The expected client IP is taken from the control connection — the SOCKS
    // server's authoritative knowledge of who the client is — not from any
    // advertised or first-seen datagram, which an attacker could forge.
    //~ implements rfc1928#7/must.9893ba
    let client_ip = stream.peer_addr()?.ip();

    // DST.ADDR/DST.PORT in the request advertise the client's UDP source port.
    // Honor the port only; the IP is fixed to the control connection above so
    // a request claiming someone else's address cannot redirect the relay.
    let advertised_port = match request.dest_addr {
        Address::V4(addr) if !addr.is_unspecified() && request.dest_port != 0 => {
            Some(request.dest_port)
        }
        Address::V6(addr) if !addr.is_unspecified() && request.dest_port != 0 => {
            Some(request.dest_port)
        }
        _ => None,
    };
    let client = advertised_port.map(|port| SocketAddr::new(client_ip, port));

    let stop = Arc::new(AtomicBool::new(false));
    let relay_stop = Arc::clone(&stop);
    let relay = thread::spawn(move || relay_datagrams(socket, client_ip, client, relay_stop));

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
/// The association is anchored to `client_ip`, fixed from the TCP control
/// connection. A datagram is classified by source:
/// - from the client's full address (IP fixed to `client_ip`; port advertised
///   or learned from its first datagram) — outbound: header unwrapped, target
///   recorded, payload forwarded;
/// - from a remote the client has actually contacted — inbound reply: wrapped
///   in a [`UdpHeader`] and returned to the client;
/// - anything else (wrong IP, or an unsolicited remote) — dropped.
///
/// Pinning the client to the control connection's IP and relaying back only
/// solicited replies prevents a third party from hijacking or injecting into
/// the association.
fn relay_datagrams(
    socket: UdpSocket,
    client_ip: IpAddr,
    advertised_client: Option<SocketAddr>,
    stop: Arc<AtomicBool>,
) -> Result<()> {
    socket.set_read_timeout(Some(Duration::from_secs(1)))?;

    let mut buf = vec![0u8; MAX_DATAGRAM];
    // Full client UDP address: IP is fixed; the port is advertised or learned
    // from the first datagram that arrives from the client IP.
    let mut client = advertised_client;
    // Remote destinations the client has sent to; only these may reply.
    let mut targets: HashSet<SocketAddr> = HashSet::new();

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

        // Classify by source: only the pinned client may originate outbound
        // traffic; only contacted remotes may reply; everything else is
        // dropped (the else-fall-through below).
        //~ implements rfc1928#7/must.977213
        let is_client = match client {
            Some(addr) => source == addr,
            // Learn the client's port from its first datagram, but only if it
            // originates from the pinned client IP.
            None => source.ip() == client_ip,
        };

        if is_client {
            client = Some(source);

            // Outbound: unwrap the header and forward the payload. Malformed
            // or fragmented datagrams are dropped per RFC 1928.
            let mut cursor = Cursor::new(&buf[..received]);
            let Ok(header) = UdpHeader::read(&mut cursor) else {
                continue;
            };
            // Fragmentation is not implemented; drop any fragmented datagram.
            //~ implements rfc1928#7/must.9cef93
            if header.frag != 0 {
                continue;
            }

            // Resolve the target (handles V4/V6/domain) so the recorded
            // addresses match the source of any reply.
            let start = cursor.position() as usize;
            let target = header.dest_addr.to_socket_string(header.dest_port);
            if let Ok(resolved) = target.to_socket_addrs() {
                let resolved: Vec<SocketAddr> = resolved.collect();
                for addr in &resolved {
                    // Bound the set; outbound still flows when at capacity.
                    if targets.len() < MAX_UDP_TARGETS {
                        targets.insert(*addr);
                    }
                }
                if let Some(addr) = resolved.first() {
                    let _ = socket.send_to(&buf[start..received], addr);
                }
            }
        } else if targets.contains(&source) {
            // Inbound reply from a remote the client contacted: wrap it with
            // the remote's address and return it to the client.
            let Some(client_addr) = client else {
                continue;
            };

            //~ implements rfc1928#7/must.641b12
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
        // Otherwise: an unsolicited or spoofed datagram — drop it.
    }
}
