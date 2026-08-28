//! Tokio netlink request transport.

use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::Arc;

use rustix::net::{
    AddressFamily, RecvFlags, SendFlags, SocketFlags, SocketType, netlink, recv, send,
};
use tokio::io::unix::AsyncFd;
use tokio::sync::Mutex;

use crate::core::{self, Attribute, Message, NLM_F_ACK, NLMSG_DONE, NLMSG_ERROR, NLMSG_NOOP};
use crate::{Error, Result};

const RECEIVE_BUFFER: usize = 1024 * 1024;
const NLMSGERR_ATTR_MSG: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Kernel netlink protocol used to open a request socket.
pub enum Protocol {
    /// Route, link, address, and rule operations.
    Route,
    /// Generic-netlink controller and families.
    Generic,
}

#[derive(Clone)]
/// Serialized Tokio request/reply transport over one netlink socket.
pub struct Client {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    socket: AsyncFd<OwnedFd>,
    port: u32,
    sequence: u32,
}

impl Client {
    /// Open, bind, connect, and enable extended acknowledgements.
    pub fn open(protocol: Protocol) -> Result<Self> {
        Self::from_socket(open_socket(protocol)?)
    }

    /// Wrap an already bound and connected nonblocking netlink socket.
    ///
    /// This is useful when the socket must be opened on a dedicated thread while
    /// that thread is temporarily entered into another network namespace.
    pub fn from_socket(socket: OwnedFd) -> Result<Self> {
        let local = rustix::net::getsockname(&socket)
            .map_err(os_error)
            .and_then(|address| {
                netlink::SocketAddrNetlink::try_from(address)
                    .map_err(|_| Error::Protocol("kernel returned a non-netlink address".into()))
            })?;
        Ok(Self {
            inner: Arc::new(Mutex::new(Inner {
                socket: AsyncFd::new(socket)?,
                port: local.pid(),
                sequence: 0,
            })),
        })
    }

    /// Send one request and collect its validated multipart response.
    pub async fn request(&self, mut message: Message) -> Result<Vec<Message>> {
        message.header.flags |= NLM_F_ACK;
        let mut inner = self.inner.lock().await;
        inner.sequence = inner.sequence.wrapping_add(1).max(1);
        let sequence = inner.sequence;
        let port = inner.port;
        let encoded = message.encode(sequence, port)?;
        send_datagram(&inner.socket, &encoded).await?;

        let mut responses = Vec::new();
        loop {
            let datagram = receive_datagram(&inner.socket).await?;
            for response in Message::decode_all(&datagram)? {
                if response.header.sequence != sequence {
                    return Err(Error::Protocol(format!(
                        "response sequence {} does not match request {sequence}",
                        response.header.sequence
                    )));
                }
                if response.header.port != 0 && response.header.port != port {
                    return Err(Error::Protocol(format!(
                        "response port {} does not match socket {port}",
                        response.header.port
                    )));
                }
                match response.header.message_type {
                    NLMSG_NOOP => {}
                    NLMSG_DONE => return Ok(responses),
                    NLMSG_ERROR => {
                        let error = parse_error(&response.payload)?;
                        if let Some(error) = error {
                            return Err(error);
                        }
                        return Ok(responses);
                    }
                    _ => responses.push(response),
                }
            }
        }
    }
}

/// Open, bind, connect, and configure a nonblocking netlink socket.
///
/// The descriptor belongs to the current network namespace at the time this
/// function runs. It can subsequently be moved to an async executor without
/// changing that association.
pub fn open_socket(protocol: Protocol) -> Result<OwnedFd> {
    let socket = rustix::net::socket_with(
        AddressFamily::NETLINK,
        SocketType::RAW,
        SocketFlags::CLOEXEC | SocketFlags::NONBLOCK,
        match protocol {
            Protocol::Route => None,
            Protocol::Generic => Some(netlink::GENERIC),
        },
    )
    .map_err(os_error)?;
    let local = netlink::SocketAddrNetlink::new(0, 0);
    rustix::net::bind(&socket, &local).map_err(os_error)?;
    let kernel = netlink::SocketAddrNetlink::new(0, 0);
    rustix::net::connect(&socket, &kernel).map_err(os_error)?;
    enable_option(&socket, libc::NETLINK_EXT_ACK)?;
    enable_option(&socket, libc::NETLINK_GET_STRICT_CHK)?;
    Ok(socket)
}

async fn send_datagram(socket: &AsyncFd<OwnedFd>, bytes: &[u8]) -> Result<()> {
    loop {
        let mut ready = socket.writable().await?;
        match send(socket.get_ref(), bytes, SendFlags::empty()) {
            Ok(sent) if sent == bytes.len() => return Ok(()),
            Ok(sent) => {
                return Err(Error::Protocol(format!(
                    "short netlink send: {sent} of {} bytes",
                    bytes.len()
                )));
            }
            Err(rustix::io::Errno::AGAIN) => ready.clear_ready(),
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(os_error(error)),
        }
    }
}

async fn receive_datagram(socket: &AsyncFd<OwnedFd>) -> Result<Vec<u8>> {
    loop {
        let mut ready = socket.readable().await?;
        let mut bytes = vec![0_u8; RECEIVE_BUFFER];
        match recv(socket.get_ref(), &mut bytes, RecvFlags::TRUNC) {
            Ok((initialized, received)) if received <= bytes.len() => {
                debug_assert_eq!(initialized, received);
                bytes.truncate(initialized);
                return Ok(bytes);
            }
            Ok((_, received)) => {
                return Err(Error::Protocol(format!(
                    "netlink datagram of {received} bytes exceeds receive limit {RECEIVE_BUFFER}"
                )));
            }
            Err(rustix::io::Errno::AGAIN) => ready.clear_ready(),
            Err(rustix::io::Errno::INTR) => {}
            Err(rustix::io::Errno::NOBUFS) => {
                return Err(Error::Protocol(
                    "netlink receive buffer overflowed; dump may be incomplete".into(),
                ));
            }
            Err(error) => return Err(os_error(error)),
        }
    }
}

fn parse_error(payload: &[u8]) -> Result<Option<Error>> {
    let code = core::read_i32(payload, 0)?;
    if code == 0 {
        return Ok(None);
    }
    let extack_offset = if payload.len() >= 4 + core::HEADER_LEN {
        let original_length = core::read_u32(payload, 4)? as usize;
        4 + core::align(original_length.min(payload.len() - 4))
    } else {
        payload.len()
    };
    let extack = payload
        .get(extack_offset..)
        .filter(|bytes| !bytes.is_empty())
        .and_then(|bytes| core::decode_attributes(bytes).ok())
        .and_then(|attributes| {
            attributes
                .into_iter()
                .find(|attribute| attribute.base_kind() == NLMSGERR_ATTR_MSG)
        })
        .and_then(|attribute: Attribute| attribute.as_string().ok());
    Ok(Some(Error::Kernel {
        errno: code.saturating_abs(),
        extack,
    }))
}

fn enable_option(socket: &OwnedFd, option: libc::c_int) -> Result<()> {
    let enabled: libc::c_int = 1;
    // SAFETY: the value points to a valid c_int and the socket remains owned.
    unsafe {
        let result = libc::setsockopt(
            socket.as_raw_fd(),
            libc::SOL_NETLINK,
            option,
            (&raw const enabled).cast(),
            std::mem::size_of_val(&enabled) as libc::socklen_t,
        );
        if result != 0 {
            return Err(Error::Io(std::io::Error::last_os_error()));
        }
    }
    Ok(())
}

fn os_error(error: rustix::io::Errno) -> Error {
    Error::Io(std::io::Error::from(error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn route_socket_opens() {
        Client::open(Protocol::Route).unwrap();
    }
}
