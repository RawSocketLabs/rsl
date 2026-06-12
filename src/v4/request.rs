use std::io::{Read, Seek, Write};
use std::net::Ipv4Addr;

use binrw::meta::{EndianKind, WriteEndian};
use binrw::{BinResult, BinWrite, Endian, NullString};
use derive_builder::Builder;

use crate::error::Result;
use crate::v4::command::Command;

/// A SOCKS4 / 4A client request.
///
/// The wire layout is a fixed 8-byte head followed by one or two
/// NULL-terminated byte strings:
///
/// ```text
/// +----+----+----+----+----+----+----+----+----+....+----+[----+....+----+]
/// | VN | CD | DSTPORT |        DSTIP      |   USERID  |NUL| [  DOMAIN  |NUL]
/// +----+----+----+----+----+----+----+----+----+....+----+[----+....+----+]
///    1    1     2 (be)        4              variable        variable  (4A)
/// ```
///
/// The trailing `DOMAIN` field exists only in **SOCKS4A** and only when the
/// client could not resolve the target itself: it then sets `DSTIP` to the
/// inadmissible `0.0.0.x` (`x` non-zero) marker and appends the destination
/// host name for the proxy to resolve. That field is compiled in only under the
/// `v4a` feature; see [`is_unresolved_marker`].
///
/// # Dual-use
///
/// `version`, `command`, and the address fields stay `pub`, and `version` is a
/// plain `u8`, so a caller can emit a deliberately wrong `VN` or an unknown
/// command for testing. Parsing ([`read_from`](Request::read_from)) is
/// correspondingly liberal — an unrecognized command becomes [`Command::Custom`]
/// rather than an error. Writing is via the [`BinWrite`] impl, which emits
/// exactly the fields the value holds (no validation).
//~ models socks4#front
#[derive(Builder, Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// VN — the SOCKS version number; 4 for a conformant request.
    /// (socks4 should.8d142d — anchored at the client that emits it.)
    #[builder(default = "4")]
    pub version: u8,

    /// CD — the command code (CONNECT or BIND).
    /// (socks4 should.01af87 / must.6e79ac — anchored at [`Command`].)
    pub command: Command,

    /// DSTPORT — the destination port (big-endian on the wire).
    pub dest_port: u16,

    /// DSTIP — the destination IPv4 address (or the `0.0.0.x` 4A marker).
    pub dest_ip: Ipv4Addr,

    /// USERID — the identity the client claims, NULL-terminated on the wire.
    /// Often empty. The terminator is added on write and stripped on read.
    #[builder(default)]
    pub userid: NullString,

    /// DOMAIN — the destination host name (SOCKS4A only), present only when
    /// `DSTIP` is the `0.0.0.x` marker. NULL-terminated on the wire.
    /// (socks4a must.a1f21e — anchored at the client that appends it.)
    #[cfg(feature = "v4a")]
    #[builder(default)]
    pub domain: Option<NullString>,
}

/// Writes the request in its exact wire form: the 8-byte head, the
/// NULL-terminated USERID, and — under `v4a`, when present — the
/// NULL-terminated DOMAIN. Emitted verbatim, with no validation, so it is
/// usable as a dual-use escape hatch.
impl WriteEndian for Request {
    // The SOCKS4 head is fixed big-endian, so `.write()` (the endian-free
    // convenience) works without the caller specifying an endianness.
    const ENDIAN: EndianKind = EndianKind::Endian(Endian::Big);
}

impl BinWrite for Request {
    type Args<'a> = ();

    fn write_options<W: Write + Seek>(
        &self,
        writer: &mut W,
        _endian: Endian,
        (): Self::Args<'_>,
    ) -> BinResult<()> {
        // The SOCKS4 head is fixed big-endian regardless of caller endianness.
        self.version.write_options(writer, Endian::Big, ())?;
        self.command.write_options(writer, Endian::Big, ())?;
        self.dest_port.write_options(writer, Endian::Big, ())?;
        self.dest_ip
            .octets()
            .write_options(writer, Endian::Big, ())?;
        self.userid.write_options(writer, Endian::Big, ())?;
        #[cfg(feature = "v4a")]
        if let Some(domain) = &self.domain {
            domain.write_options(writer, Endian::Big, ())?;
        }
        Ok(())
    }
}

/// Whether a `DSTIP` is the SOCKS4A "could not resolve" marker: `0.0.0.x` with
/// a non-zero final octet. Per the 4A memo such an address is inadmissible as a
/// real destination, so it unambiguously signals that a host name follows.
//~ implements socks4a#front/should.f6d704 part="0.0.0.x marker test"
//~ implements socks4a#front/must.febee7 part="server checks DSTIP"
pub fn is_unresolved_marker(ip: &Ipv4Addr) -> bool {
    let [a, b, c, d] = ip.octets();
    a == 0 && b == 0 && c == 0 && d != 0
}

impl Request {
    /// Reads a `Request` from a byte-oriented stream, consuming exactly the
    /// bytes that belong to the message (the 8-byte head, the NULL-terminated
    /// USERID, and — under `v4a`, when `DSTIP` is the marker — the
    /// NULL-terminated domain). No trailing bytes are read.
    ///
    /// # Errors
    /// Returns an error if I/O fails reading any field.
    //~ implements socks4a#front/must.04b317 part="read domain when marker present"
    pub fn read_from(reader: &mut impl Read) -> Result<Self> {
        let mut head = [0u8; 8];
        reader.read_exact(&mut head)?;

        let version = head[0];
        let command = parse_command(head[1]);
        let dest_port = u16::from_be_bytes([head[2], head[3]]);
        let dest_ip = Ipv4Addr::new(head[4], head[5], head[6], head[7]);

        let userid = NullString(read_null_terminated(reader)?);

        // The marker branch is the only place a domain may follow; outside
        // `v4a` we never read it, so a plain-v4 server treats `0.0.0.x` as an
        // ordinary (if unusual) destination address.
        #[cfg(feature = "v4a")]
        let domain = if is_unresolved_marker(&dest_ip) {
            Some(NullString(read_null_terminated(reader)?))
        } else {
            None
        };

        Ok(Request {
            version,
            command,
            dest_port,
            dest_ip,
            userid,
            #[cfg(feature = "v4a")]
            domain,
        })
    }
}

/// Maps a raw command byte to a [`Command`], preserving unknown codes.
fn parse_command(byte: u8) -> Command {
    match byte {
        1 => Command::Connect,
        2 => Command::Bind,
        other => Command::Custom(other),
    }
}

/// Reads bytes one at a time until (and consuming) a terminating NULL,
/// returning the content without the terminator. Reading byte-by-byte keeps the
/// framing exact: nothing past the terminator is consumed from the stream.
fn read_null_terminated(reader: &mut impl Read) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        reader.read_exact(&mut byte)?;
        if byte[0] == 0 {
            return Ok(out);
        }
        out.push(byte[0]);
    }
}

#[cfg(test)]
mod unit {
    use binrw::io::Cursor;

    use super::*;

    #[test]
    fn marker_detection() {
        assert!(is_unresolved_marker(&Ipv4Addr::new(0, 0, 0, 1)));
        assert!(is_unresolved_marker(&Ipv4Addr::new(0, 0, 0, 255)));
        assert!(!is_unresolved_marker(&Ipv4Addr::new(0, 0, 0, 0)));
        assert!(!is_unresolved_marker(&Ipv4Addr::new(1, 0, 0, 1)));
        assert!(!is_unresolved_marker(&Ipv4Addr::new(127, 0, 0, 1)));
    }

    #[test]
    fn command_parsing_is_liberal() {
        assert_eq!(parse_command(1), Command::Connect);
        assert_eq!(parse_command(2), Command::Bind);
        assert_eq!(parse_command(9), Command::Custom(9));
    }

    #[test]
    fn connect_request_round_trips_with_userid() {
        let request = RequestBuilder::default()
            .command(Command::Connect)
            .dest_port(80)
            .dest_ip(Ipv4Addr::new(93, 184, 216, 34))
            .userid(NullString::from("alice"))
            .build()
            .unwrap();

        let mut cursor = Cursor::new(Vec::new());
        request.write(&mut cursor).unwrap();
        let mut bytes = cursor.into_inner();
        // VN=4, CD=1, port 80, ip, "alice", NUL.
        assert_eq!(&bytes[..2], &[4, 1]);
        assert_eq!(&bytes[2..4], &80u16.to_be_bytes());
        assert_eq!(&bytes[4..8], &[93, 184, 216, 34]);
        assert_eq!(&bytes[8..], b"alice\0");

        bytes.extend_from_slice(b"trailing");
        let mut reader = bytes.as_slice();
        let parsed = Request::read_from(&mut reader).expect("parses");
        assert_eq!(parsed.command, Command::Connect);
        assert_eq!(parsed.dest_port, 80);
        assert_eq!(parsed.userid.to_string(), "alice");
        assert_eq!(reader, b"trailing", "framing must not over-read");
    }

    #[cfg(feature = "v4a")]
    #[test]
    fn socks4a_request_carries_domain_after_marker() {
        let request = RequestBuilder::default()
            .command(Command::Connect)
            .dest_port(443)
            .dest_ip(Ipv4Addr::new(0, 0, 0, 7))
            .userid(NullString::default())
            .domain(Some(NullString::from("example.com")))
            .build()
            .unwrap();

        let mut cursor = Cursor::new(Vec::new());
        request.write(&mut cursor).unwrap();
        let mut bytes = cursor.into_inner();
        // head(8) + NUL userid + "example.com" + NUL.
        assert_eq!(&bytes[4..8], &[0, 0, 0, 7]);
        assert_eq!(&bytes[8..], b"\0example.com\0");

        bytes.push(0x42); // a stray trailing byte
        let mut reader = bytes.as_slice();
        let parsed = Request::read_from(&mut reader).expect("parses");
        assert_eq!(
            parsed.domain.as_ref().map(|d| d.to_string()),
            Some("example.com".to_string())
        );
        assert_eq!(
            reader,
            &[0x42],
            "framing must stop at the domain terminator"
        );
    }

    #[cfg(feature = "v4a")]
    #[test]
    fn socks4a_ip_target_has_no_domain() {
        // A real DSTIP (not the marker) means no trailing domain is read.
        let request = RequestBuilder::default()
            .command(Command::Connect)
            .dest_port(80)
            .dest_ip(Ipv4Addr::new(10, 0, 0, 1))
            .build()
            .unwrap();
        let mut c = Cursor::new(Vec::new());
        request.write(&mut c).unwrap();
        let bytes = c.into_inner();

        let parsed = Request::read_from(&mut bytes.as_slice()).unwrap();
        assert_eq!(parsed.dest_ip, Ipv4Addr::new(10, 0, 0, 1));
        assert_eq!(parsed.domain, None);
    }
}
