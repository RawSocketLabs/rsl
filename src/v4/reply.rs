use std::io::Read;
use std::net::{Ipv4Addr, SocketAddrV4};

use binrw::{binrw, io::Cursor, BinRead};
use derive_builder::Builder;

use crate::error::Result;
use crate::v4::reply_code::ReplyCode;

/// A SOCKS4 / 4A server reply — a fixed 8-byte message.
///
/// ```text
/// +----+----+----+----+----+----+----+----+
/// | VN | CD | DSTPORT |        DSTIP      |
/// +----+----+----+----+----+----+----+----+
///    1    1     2 (be)        4
/// ```
///
/// `VN` is 0 in a conformant reply (note: *not* 4 — a deliberate quirk of the
/// protocol). `CD` is the [`ReplyCode`]. For BIND the `DSTIP`/`DSTPORT` carry
/// the address the proxy is listening on; if `DSTIP` is `0.0.0.0`
/// (`INADDR_ANY`) the client substitutes the proxy's own address (see
/// [`Reply::bound_socket`]).
//~ models socks4#front
#[binrw]
#[brw(big)]
#[derive(Builder, Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    /// VN — the version of the reply code; 0 in a conformant reply.
    /// (socks4 should.6ba535 — anchored at the server's `send_reply`.)
    #[builder(default = "0")]
    pub version: u8,

    /// CD — the result code.
    pub code: ReplyCode,

    /// DSTPORT — for BIND, the port the proxy bound; otherwise echoes the
    /// request (often zero on failure).
    #[builder(default = "0")]
    pub dest_port: u16,

    /// DSTIP — for BIND, the address the proxy bound (possibly `INADDR_ANY`).
    #[br(map = |b: [u8; 4]| Ipv4Addr::from(b))]
    #[bw(map = |ip: &Ipv4Addr| ip.octets())]
    #[builder(default = "Ipv4Addr::UNSPECIFIED")]
    pub dest_ip: Ipv4Addr,
}

impl Reply {
    /// Reads the fixed 8-byte reply from a stream.
    ///
    /// # Errors
    /// Returns an error if I/O fails or the message cannot be parsed.
    pub fn read_from(reader: &mut impl Read) -> Result<Self> {
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf)?;
        Self::read(&mut Cursor::new(buf)).map_err(Into::into)
    }

    /// The bound socket address this reply names, substituting `fallback` for
    /// the proxy's own address when `DSTIP` is `INADDR_ANY` (`0.0.0.0`) — the
    /// memo directs the client to do exactly this for BIND replies.
    //~ implements socks4#front/should.c1f159 part="substitute proxy IP for INADDR_ANY"
    pub fn bound_socket(&self, fallback: Ipv4Addr) -> SocketAddrV4 {
        let ip = if self.dest_ip.is_unspecified() {
            fallback
        } else {
            self.dest_ip
        };
        SocketAddrV4::new(ip, self.dest_port)
    }
}

#[cfg(test)]
mod unit {
    use binrw::BinWrite;

    use super::*;

    #[test]
    fn reply_round_trips() {
        let reply = ReplyBuilder::default()
            .code(ReplyCode::Granted)
            .dest_port(1080)
            .dest_ip(Ipv4Addr::new(127, 0, 0, 1))
            .build()
            .unwrap();

        let mut cursor = Cursor::new(Vec::new());
        reply.write(&mut cursor).unwrap();
        let bytes = cursor.into_inner();
        assert_eq!(bytes, vec![0, 90, 0x04, 0x38, 127, 0, 0, 1]);

        let parsed = Reply::read_from(&mut bytes.as_slice()).unwrap();
        assert_eq!(parsed, reply);
    }

    #[test]
    fn bound_socket_substitutes_proxy_for_inaddr_any() {
        let proxy = Ipv4Addr::new(198, 51, 100, 7);
        let any = ReplyBuilder::default()
            .code(ReplyCode::Granted)
            .dest_port(40000)
            .build()
            .unwrap();
        assert_eq!(
            any.bound_socket(proxy),
            SocketAddrV4::new(proxy, 40000)
        );

        let concrete = ReplyBuilder::default()
            .code(ReplyCode::Granted)
            .dest_port(40000)
            .dest_ip(Ipv4Addr::new(203, 0, 113, 9))
            .build()
            .unwrap();
        assert_eq!(
            concrete.bound_socket(proxy),
            SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 9), 40000)
        );
    }
}
