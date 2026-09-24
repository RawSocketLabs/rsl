use std::net::{Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use crate::error::{Result, TftpError};
use crate::netascii;
use crate::server::handler::{Handler, Reject};
#[cfg(feature = "options")]
use crate::transfer::recv_packet;
use crate::transfer::{
    BLOCK_SIZE, DEFAULT_MAX_RETRIES, DEFAULT_TIMEOUT, RECV_BUFFER, recv_file, send_error, send_file,
};
use crate::wire::{Ack, Packet, Request, RequestKind};

#[cfg(feature = "options")]
use crate::wire::{self, OptionAck, TftpOption};

/// Default cap on concurrent transfers in [`Server::serve`].
pub const DEFAULT_MAX_CONNECTIONS: usize = 1024;

/// A TFTP server (RFC 1350).
///
/// [`bind`](Self::bind) it to a listen address (port 69 by convention) with a
/// [`Handler`] backend, then drive it with [`serve`](Self::serve) (loop, one
/// thread per transfer) or [`serve_one`](Self::serve_one) (one transfer, current
/// thread). Each transfer is handled on a freshly-bound socket — its own
/// transfer ID — exactly as RFC 1350 §4 requires.
///
/// # Example
///
/// ```no_run
/// use std::sync::Arc;
/// use tftp::server::{MemoryStore, Server};
///
/// # fn main() -> Result<(), tftp::error::TftpError> {
/// let store = Arc::new(MemoryStore::new().with_file("motd.txt", b"hello\n"));
/// Server::bind("0.0.0.0:69", store)?.serve()?;
/// # Ok(())
/// # }
/// ```
//~ models rfc1350#2
pub struct Server {
    socket: UdpSocket,
    handler: Arc<dyn Handler>,
    timeout: Duration,
    max_retries: u32,
    max_connections: usize,
}

impl Server {
    /// Binds the listen socket and attaches the [`Handler`] backend.
    ///
    /// # Errors
    /// Returns an error if the socket cannot be bound.
    pub fn bind(addr: impl ToSocketAddrs, handler: Arc<dyn Handler>) -> Result<Self> {
        Ok(Self {
            socket: UdpSocket::bind(addr)?,
            handler,
            timeout: DEFAULT_TIMEOUT,
            max_retries: DEFAULT_MAX_RETRIES,
            max_connections: DEFAULT_MAX_CONNECTIONS,
        })
    }

    /// Sets the per-packet retransmission timeout for transfers.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets the number of retransmissions before a transfer is abandoned.
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Sets the maximum number of concurrent transfers in [`serve`](Self::serve).
    /// Excess requests wait in the socket buffer until a slot frees. Minimum 1.
    pub fn with_max_connections(mut self, max: usize) -> Self {
        self.max_connections = max.max(1);
        self
    }

    /// The address the listener is bound to.
    ///
    /// # Errors
    /// Returns an error if the socket address cannot be retrieved.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.socket.local_addr()?)
    }

    /// Accepts and fully handles a single transfer on the current thread.
    ///
    /// # Errors
    /// Returns an error if receiving the request fails, the datagram is not a
    /// request, or the transfer ends in an error.
    pub fn serve_one(&self) -> Result<()> {
        let mut buf = vec![0u8; RECV_BUFFER];
        let (n, client) = self.socket.recv_from(&mut buf)?;
        let request = decode_request(&buf[..n])?;
        handle_request(
            &*self.handler,
            client,
            request,
            self.timeout,
            self.max_retries,
        )
    }

    /// Serves transfers until the listener fails, one thread per transfer, up to
    /// [`max_connections`](Self::with_max_connections) concurrently.
    ///
    /// # Errors
    /// Returns an error if receiving on the listen socket fails.
    pub fn serve(&self) -> Result<()> {
        let slots = Semaphore::new(self.max_connections);
        let mut buf = vec![0u8; RECV_BUFFER];
        loop {
            let (n, client) = self.socket.recv_from(&mut buf)?;
            let request = match decode_request(&buf[..n]) {
                Ok(request) => request,
                Err(err) => {
                    tracing::warn!(%client, %err, "ignoring non-request datagram on listen socket");
                    continue;
                }
            };
            let permit = slots.acquire();
            let handler = Arc::clone(&self.handler);
            let timeout = self.timeout;
            let max_retries = self.max_retries;
            thread::spawn(move || {
                let _permit = permit; // released when the transfer thread ends
                if let Err(err) = handle_request(&*handler, client, request, timeout, max_retries) {
                    tracing::debug!(%client, %err, "transfer ended with error");
                }
            });
        }
    }
}

/// Decodes a datagram that must be an RRQ/WRQ.
fn decode_request(buf: &[u8]) -> Result<Request> {
    match Packet::decode(buf)? {
        Packet::Request(request) => Ok(request),
        other => Err(TftpError::Unexpected(format!(
            "expected a request on the listen socket, got {:?}",
            other.opcode()
        ))),
    }
}

/// Handles one transfer on a freshly-bound socket (a new server TID), dispatching
/// by request kind.
#[tracing::instrument(name = "tftp.serve", skip(handler, request), fields(%client, kind = ?request.kind))]
fn handle_request(
    handler: &dyn Handler,
    client: SocketAddr,
    request: Request,
    timeout: Duration,
    max_retries: u32,
) -> Result<()> {
    // A new ephemeral socket is this transfer's TID (RFC 1350 §4).
    //~ implements rfc1350#4/should.45bc6d part="fresh server TID per transfer"
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    socket.set_read_timeout(Some(timeout))?;

    let filename = request.filename_lossy();
    tracing::debug!(filename, "handling request");
    match request.kind {
        RequestKind::Read => handle_rrq(&socket, handler, client, &request, max_retries),
        RequestKind::Write => handle_wrq(&socket, handler, client, &request, max_retries),
    }
}

/// Serves a download: fetch the file from the handler, (optionally) negotiate
/// options, then send the DATA stream.
fn handle_rrq(
    socket: &UdpSocket,
    handler: &dyn Handler,
    client: SocketAddr,
    request: &Request,
    max_retries: u32,
) -> Result<()> {
    let filename = request.filename_lossy();
    let raw = match handler.read(&filename) {
        Ok(bytes) => bytes,
        Err(reject) => return reject_transfer(socket, client, reject),
    };
    let payload = if request.mode.is_netascii() {
        netascii::to_netascii(&raw)
    } else {
        raw
    };

    #[cfg_attr(not(feature = "options"), allow(unused_mut))]
    let mut blksize = BLOCK_SIZE;

    #[cfg(feature = "options")]
    {
        // On a read, `tsize` is answered with the real file size (RFC 2349).
        let accepted = accept_options(&request.options, Some(payload.len() as u64));
        if !accepted.is_empty() {
            if let Some(bs) = wire::blksize(&accepted) {
                blksize = bs as usize;
            }
            let oack = OptionAck::new(accepted).encode();
            socket.send_to(&oack, client)?;
            //~ implements rfc2347#front/may.2a0304 part="server answers options with an OACK"
            await_ack0(socket, client, &oack, max_retries)?;
        }
    }

    send_file(socket, client, &payload, blksize, max_retries)
}

/// Serves an upload: authorize, (optionally) negotiate options, receive the DATA
/// stream, then store it.
fn handle_wrq(
    socket: &UdpSocket,
    handler: &dyn Handler,
    client: SocketAddr,
    request: &Request,
    max_retries: u32,
) -> Result<()> {
    let filename = request.filename_lossy();
    // Authorize before any data is transferred — the correct point to report
    // "file exists" / "access violation" (RFC 1350 §7).
    if let Err(reject) = handler.authorize_write(&filename) {
        return reject_transfer(socket, client, reject);
    }

    #[cfg_attr(not(feature = "options"), allow(unused_mut))]
    let mut blksize = BLOCK_SIZE;

    // Prime the transfer: send the OACK (if options were accepted) or ACK(0).
    let primed = {
        #[cfg(feature = "options")]
        {
            // On a write, `tsize` is echoed back to confirm it (RFC 2349).
            let echo_tsize = wire::tsize(&request.options);
            let accepted = accept_options(&request.options, echo_tsize);
            if accepted.is_empty() {
                let ack = Ack::new(0).encode();
                socket.send_to(&ack, client)?;
                ack
            } else {
                if let Some(bs) = wire::blksize(&accepted) {
                    blksize = bs as usize;
                }
                let oack = OptionAck::new(accepted).encode();
                socket.send_to(&oack, client)?;
                oack
            }
        }
        #[cfg(not(feature = "options"))]
        {
            let ack = Ack::new(0).encode();
            socket.send_to(&ack, client)?;
            ack
        }
    };

    let mut data = Vec::new();
    recv_file(socket, client, 1, blksize, max_retries, primed, &mut data)?;

    let contents = if request.mode.is_netascii() {
        netascii::from_netascii(&data)
    } else {
        data
    };
    if let Err(reject) = handler.write(&filename, contents) {
        return reject_transfer(socket, client, reject);
    }
    Ok(())
}

/// Sends an ERROR for a handler rejection and ends the transfer.
fn reject_transfer(socket: &UdpSocket, client: SocketAddr, reject: Reject) -> Result<()> {
    tracing::debug!(code = ?reject.code, message = %reject.message, "rejecting transfer");
    send_error(socket, client, reject.code, &reject.message);
    Err(TftpError::Peer {
        code: reject.code,
        message: reject.message,
    })
}

/// Waits for the client's ACK(0) that follows an OACK on a read, retransmitting
/// the OACK on timeout.
#[cfg(feature = "options")]
fn await_ack0(socket: &UdpSocket, client: SocketAddr, oack: &[u8], max_retries: u32) -> Result<()> {
    let mut buf = vec![0u8; RECV_BUFFER];
    let (packet, _) = recv_packet(socket, &mut buf, Some(client), oack, client, max_retries)?;
    match packet {
        Packet::Ack(a) if a.block == 0 => Ok(()),
        Packet::Error(err) => Err(TftpError::from_error_packet(&err)),
        other => Err(TftpError::Unexpected(format!(
            "expected ACK(0) after OACK, got {:?}",
            other.opcode()
        ))),
    }
}

/// Selects which requested options to acknowledge: known options are accepted as
/// requested, `tsize` takes `tsize_response`, and unsupported options are
/// omitted (never an error, per RFC 2347).
//~ implements rfc2347#front/must-not.f25971 part="only acknowledge requested options"
//~ implements rfc2347#front/should-not.8c01fb part="omit unsupported options, no error"
//~ implements rfc2347#front/should.056822 part="omit invalid/unsupported option values"
//~ implements rfc2348#front/must.64cfec part="acked blksize <= the requested value (echoed)"
//~ implements rfc2349#front/must.e2bfbc part="acked timeout matches the requested value"
#[cfg(feature = "options")]
fn accept_options(requested: &[TftpOption], tsize_response: Option<u64>) -> Vec<TftpOption> {
    let mut accepted = Vec::new();
    for option in requested {
        match option {
            TftpOption::BlkSize(n) => accepted.push(TftpOption::BlkSize(*n)),
            TftpOption::Timeout(n) => accepted.push(TftpOption::Timeout(*n)),
            TftpOption::TSize(_) => {
                if let Some(size) = tsize_response {
                    accepted.push(TftpOption::TSize(size));
                }
            }
            // Unsupported options are silently dropped from the OACK.
            TftpOption::Other { .. } => {}
        }
    }
    accepted
}

/// A counting semaphore bounding concurrent transfer threads (the same guard the
/// SOCKS server uses): a flood of requests cannot spawn unbounded threads.
struct Semaphore {
    permits: Mutex<usize>,
    available: Condvar,
}

impl Semaphore {
    fn new(permits: usize) -> Arc<Self> {
        Arc::new(Self {
            permits: Mutex::new(permits),
            available: Condvar::new(),
        })
    }

    fn acquire(self: &Arc<Self>) -> Permit {
        let mut permits = self.permits.lock().unwrap();
        while *permits == 0 {
            permits = self.available.wait(permits).unwrap();
        }
        *permits -= 1;
        Permit(Arc::clone(self))
    }
}

struct Permit(Arc<Semaphore>);

impl Drop for Permit {
    fn drop(&mut self) {
        *self.0.permits.lock().unwrap() += 1;
        self.0.available.notify_one();
    }
}

#[cfg(test)]
mod unit {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn semaphore_blocks_at_capacity_and_releases_on_drop() {
        let sem = Semaphore::new(1);
        let held = sem.acquire();

        let (tx, rx) = mpsc::channel();
        let waiter = {
            let sem = Arc::clone(&sem);
            thread::spawn(move || {
                let _permit = sem.acquire();
                tx.send(()).unwrap();
            })
        };

        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
        drop(held);
        assert!(rx.recv_timeout(Duration::from_secs(2)).is_ok());
        waiter.join().unwrap();
    }
}
