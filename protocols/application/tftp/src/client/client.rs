use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use crate::error::{Result, TftpError};
use crate::netascii;
use crate::transfer::{
    BLOCK_SIZE, DEFAULT_MAX_RETRIES, DEFAULT_TIMEOUT, RECV_BUFFER, recv_file, recv_packet,
    send_file,
};
use crate::wire::{Ack, Packet, Request, TransferMode};

#[cfg(feature = "options")]
use crate::wire::{self, TftpOption};

/// A high-level TFTP client.
///
/// Construct one with the server address, optionally tune the transfer
/// ([`mode`](Self::mode), [`with_timeout`](Self::with_timeout),
/// [`with_max_retries`](Self::with_max_retries), and — with the `options`
/// feature — [`block_size`](Self::block_size) etc.), then call
/// [`get`](Self::get) to download or [`put`](Self::put) to upload. Each call
/// runs a complete transfer on its own freshly-bound UDP socket (its own TID),
/// so a single `Client` is a reusable description of how to reach the server,
/// not a live connection.
///
/// # Example
///
/// ```no_run
/// use tftp::client::Client;
/// use tftp::wire::TransferMode;
///
/// # fn main() -> Result<(), tftp::error::TftpError> {
/// let client = Client::new("203.0.113.10:69".parse()?).mode(TransferMode::NetAscii);
/// let text = client.get("motd.txt")?;
/// # let _ = text;
/// # Ok(())
/// # }
/// ```
//~ models rfc1350#2
#[derive(Clone, Debug)]
pub struct Client {
    /// The server's address (port 69 by convention for the initial request).
    pub server: SocketAddr,
    mode: TransferMode,
    timeout: Duration,
    max_retries: u32,
    #[cfg(feature = "options")]
    block_size: Option<u16>,
    #[cfg(feature = "options")]
    timeout_option: Option<u8>,
    #[cfg(feature = "options")]
    request_tsize: bool,
}

impl Client {
    /// Creates a client that talks to `server`, transferring in `octet` mode.
    pub fn new(server: SocketAddr) -> Self {
        Self {
            server,
            mode: TransferMode::Octet,
            timeout: DEFAULT_TIMEOUT,
            max_retries: DEFAULT_MAX_RETRIES,
            #[cfg(feature = "options")]
            block_size: None,
            #[cfg(feature = "options")]
            timeout_option: None,
            #[cfg(feature = "options")]
            request_tsize: false,
        }
    }

    /// Sets the transfer mode (`octet` by default; `netascii` applies
    /// line-ending translation).
    pub fn mode(mut self, mode: TransferMode) -> Self {
        self.mode = mode;
        self
    }

    /// Sets the per-packet retransmission timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets the number of retransmissions before a transfer is abandoned.
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Requests a negotiated block size (RFC 2348). The server may accept it
    /// (DATA blocks then use this size) or ignore it (the transfer falls back to
    /// 512). Valid range is 8..=65464.
    #[cfg(feature = "options")]
    pub fn block_size(mut self, blksize: u16) -> Self {
        self.block_size = Some(blksize);
        self
    }

    /// Requests a negotiated retransmission timeout in seconds (RFC 2349).
    #[cfg(feature = "options")]
    pub fn timeout_option(mut self, seconds: u8) -> Self {
        self.timeout_option = Some(seconds);
        self
    }

    /// Requests transfer-size negotiation (RFC 2349 `tsize`): on a download the
    /// server reports the file size, on an upload the client announces it.
    #[cfg(feature = "options")]
    pub fn request_tsize(mut self, request: bool) -> Self {
        self.request_tsize = request;
        self
    }

    /// Downloads `filename` from the server (a Read Request), returning its
    /// bytes. In `netascii` mode the received stream is translated to host text.
    ///
    /// # Errors
    /// Returns [`TftpError::Peer`] if the server reports an error (e.g. file not
    /// found), [`TftpError::TimedOut`] if the transfer stalls, or an I/O or
    /// decode error.
    #[tracing::instrument(name = "tftp.get", skip(self), fields(server = %self.server))]
    pub fn get(&self, filename: &str) -> Result<Vec<u8>> {
        validate_filename(filename)?;
        let socket = self.bind()?;

        let request = self.attach_options(Request::read(filename, self.mode.clone()), 0);
        let last_sent = request.encode();
        socket.send_to(&last_sent, self.server)?;
        tracing::debug!(filename, "sent RRQ");

        let mut buf = vec![0u8; RECV_BUFFER];
        #[cfg_attr(not(feature = "options"), allow(unused_mut))]
        let mut blksize = BLOCK_SIZE;
        let mut data = Vec::new();

        // First response: the server replies from a fresh TID, which we lock.
        //~ implements rfc1350#4/should.45bc6d part="server TID locked from first reply"
        let (packet, server_tid) = recv_packet(
            &socket,
            &mut buf,
            None,
            &last_sent,
            self.server,
            self.max_retries,
        )?;

        // Resolve the first packet into (the next block we expect, the ACK we
        // just sent), or finish early if the very first DATA block is short.
        let (start_block, primed_ack) = match packet {
            #[cfg(feature = "options")]
            Packet::OptionAck(oack) => {
                self.apply_oack(&oack, &socket, &mut blksize)?;
                let ack = Ack::new(0).encode();
                socket.send_to(&ack, server_tid)?;
                (1, ack)
            }
            Packet::Data(d) if d.block == 1 => {
                data.extend_from_slice(&d.payload);
                let is_final = d.payload.len() < blksize;
                let ack = Ack::new(1).encode();
                socket.send_to(&ack, server_tid)?;
                if is_final {
                    return Ok(self.finish_download(data));
                }
                (2, ack)
            }
            Packet::Error(err) => return Err(TftpError::from_error_packet(&err)),
            other => {
                return Err(TftpError::Unexpected(format!(
                    "expected DATA/OACK to start a download, got {:?}",
                    other.opcode()
                )));
            }
        };

        recv_file(
            &socket,
            server_tid,
            start_block,
            blksize,
            self.max_retries,
            primed_ack,
            &mut data,
        )?;
        tracing::debug!(bytes = data.len(), "download complete");
        Ok(self.finish_download(data))
    }

    /// Uploads `contents` to the server as `filename` (a Write Request). In
    /// `netascii` mode the bytes are translated to the wire format first.
    ///
    /// # Errors
    /// Returns [`TftpError::Peer`] if the server refuses (e.g. access violation,
    /// file exists), [`TftpError::TimedOut`] if the transfer stalls, or an I/O
    /// or decode error.
    #[tracing::instrument(name = "tftp.put", skip(self, contents), fields(server = %self.server, len = contents.len()))]
    pub fn put(&self, filename: &str, contents: &[u8]) -> Result<()> {
        validate_filename(filename)?;
        let socket = self.bind()?;

        let payload = if self.mode.is_netascii() {
            netascii::to_netascii(contents)
        } else {
            contents.to_vec()
        };

        let mut request = Request::write(filename, self.mode.clone());
        request = self.attach_options(request, payload.len());
        let last_sent = request.encode();
        socket.send_to(&last_sent, self.server)?;
        tracing::debug!(filename, "sent WRQ");

        let mut buf = vec![0u8; RECV_BUFFER];
        #[cfg_attr(not(feature = "options"), allow(unused_mut))]
        let mut blksize = BLOCK_SIZE;

        // First response: ACK(0) (or OACK with the `options` feature). Lock TID.
        //~ implements rfc2347#front/may.acd06f part="handle the WRQ option-negotiation responses"
        let (packet, server_tid) = recv_packet(
            &socket,
            &mut buf,
            None,
            &last_sent,
            self.server,
            self.max_retries,
        )?;
        match packet {
            #[cfg(feature = "options")]
            Packet::OptionAck(oack) => self.apply_oack(&oack, &socket, &mut blksize)?,
            Packet::Ack(a) if a.block == 0 => {}
            Packet::Error(err) => return Err(TftpError::from_error_packet(&err)),
            other => {
                return Err(TftpError::Unexpected(format!(
                    "expected ACK(0)/OACK to start an upload, got {:?}",
                    other.opcode()
                )));
            }
        }

        send_file(&socket, server_tid, &payload, blksize, self.max_retries)?;
        tracing::debug!(bytes = payload.len(), "upload complete");
        Ok(())
    }

    /// Binds a fresh UDP socket (the client's TID) with the read timeout set.
    fn bind(&self) -> Result<UdpSocket> {
        let socket = UdpSocket::bind((std::net::Ipv4Addr::UNSPECIFIED, 0))?;
        socket.set_read_timeout(Some(self.timeout))?;
        Ok(socket)
    }

    /// Translates the accumulated download to host text when in `netascii` mode.
    fn finish_download(&self, data: Vec<u8>) -> Vec<u8> {
        if self.mode.is_netascii() {
            netascii::from_netascii(&data)
        } else {
            data
        }
    }

    #[cfg(feature = "options")]
    fn attach_options(&self, request: Request, write_size: usize) -> Request {
        let mut options = Vec::new();
        if let Some(bs) = self.block_size {
            options.push(TftpOption::BlkSize(bs));
        }
        if let Some(to) = self.timeout_option {
            options.push(TftpOption::Timeout(to));
        }
        if self.request_tsize {
            // On a write we announce the size; on a read we send 0 to ask.
            let size = if request.kind == crate::wire::RequestKind::Write {
                write_size as u64
            } else {
                0
            };
            options.push(TftpOption::TSize(size));
        }
        if options.is_empty() {
            request
        } else {
            //~ implements rfc2347#front/may.328510 part="client appends options to the request"
            request.with_options(options)
        }
    }

    #[cfg(not(feature = "options"))]
    fn attach_options(&self, request: Request, _write_size: usize) -> Request {
        request
    }

    /// Applies a server OACK: adopt the accepted block size and timeout.
    //~ implements rfc2347#front/must-not.74b0e5 part="use only acknowledged options"
    //~ implements rfc2347#front/must.618b75 part="an unacknowledged option is ignored"
    //~ implements rfc2348#front/must.90da13 part="client uses the OACK-specified blksize"
    #[cfg(feature = "options")]
    fn apply_oack(
        &self,
        oack: &crate::wire::OptionAck,
        socket: &UdpSocket,
        blksize: &mut usize,
    ) -> Result<()> {
        if let Some(bs) = wire::blksize(&oack.options) {
            *blksize = bs as usize;
            tracing::debug!(blksize = bs, "server accepted blksize");
        }
        if let Some(secs) = wire::timeout_secs(&oack.options) {
            socket.set_read_timeout(Some(Duration::from_secs(secs as u64)))?;
        }
        Ok(())
    }
}

/// Rejects an empty filename before any I/O (a basic local precondition).
fn validate_filename(filename: &str) -> Result<()> {
    if filename.is_empty() {
        return Err(TftpError::Validation("filename must not be empty".into()));
    }
    Ok(())
}
