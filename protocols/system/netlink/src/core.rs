//! Runtime-neutral netlink and netlink-attribute codecs.

use crate::{Error, Result};
use bnb::bin;

#[cfg(target_endian = "little")]
#[bin(little, bits = lsb, no_builder)]
struct HeaderWire {
    length: u32,
    message_type: u16,
    flags: u16,
    sequence: u32,
    port: u32,
}

#[cfg(target_endian = "big")]
#[bin(big, bits = msb, no_builder)]
struct HeaderWire {
    length: u32,
    message_type: u16,
    flags: u16,
    sequence: u32,
    port: u32,
}

#[cfg(target_endian = "little")]
#[bin(little, bits = lsb, no_builder)]
struct AttributeHeaderWire {
    length: u16,
    kind: u16,
}

#[cfg(target_endian = "big")]
#[bin(big, bits = msb, no_builder)]
struct AttributeHeaderWire {
    length: u16,
    kind: u16,
}

/// Byte length of `nlmsghdr`.
pub const HEADER_LEN: usize = 16;
/// Byte length of `nlattr`.
pub const ATTRIBUTE_HEADER_LEN: usize = 4;
/// Kernel alignment for messages and attributes.
pub const ALIGNMENT: usize = 4;
/// Attribute flag marking a nested attribute list.
pub const NLA_F_NESTED: u16 = 1 << 15;
/// Attribute flag marking a network-byte-order payload.
pub const NLA_F_NET_BYTEORDER: u16 = 1 << 14;
/// Mask selecting an attribute's numeric type.
pub const NLA_TYPE_MASK: u16 = !(NLA_F_NESTED | NLA_F_NET_BYTEORDER);

/// No-op message type.
pub const NLMSG_NOOP: u16 = 1;
/// Acknowledgement or kernel-error message type.
pub const NLMSG_ERROR: u16 = 2;
/// Multipart terminator message type.
pub const NLMSG_DONE: u16 = 3;

/// Marks a message as a request.
pub const NLM_F_REQUEST: u16 = 0x0001;
/// Marks one element of a multipart response.
pub const NLM_F_MULTI: u16 = 0x0002;
/// Requests an explicit acknowledgement.
pub const NLM_F_ACK: u16 = 0x0004;
/// Requests that the kernel echo the resulting object.
pub const NLM_F_ECHO: u16 = 0x0008;
/// Selects the root of a dump.
pub const NLM_F_ROOT: u16 = 0x0100;
/// Selects matching objects in a dump.
pub const NLM_F_MATCH: u16 = 0x0200;
/// Requests a complete dump.
pub const NLM_F_DUMP: u16 = NLM_F_ROOT | NLM_F_MATCH;
/// Replaces an existing object when used on a create request.
pub const NLM_F_REPLACE: u16 = 0x0100;
/// Requires that an object not already exist.
pub const NLM_F_EXCL: u16 = 0x0200;
/// Creates an object when it does not exist.
pub const NLM_F_CREATE: u16 = 0x0400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Decoded native-endian netlink message header.
pub struct Header {
    /// Unaligned message length including the header.
    pub length: u32,
    /// Protocol-specific message type.
    pub message_type: u16,
    /// `NLM_F_*` flags.
    pub flags: u16,
    /// Request/reply correlation sequence.
    pub sequence: u32,
    /// Netlink port identifier.
    pub port: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// One decoded netlink message and its untyped protocol payload.
pub struct Message {
    /// Decoded header.
    pub header: Header,
    /// Bytes following the header and preceding alignment padding.
    pub payload: Vec<u8>,
}

impl Message {
    /// Construct a message whose sequence and port are assigned by a transport.
    pub fn new(message_type: u16, flags: u16, payload: Vec<u8>) -> Self {
        Self {
            header: Header {
                length: 0,
                message_type,
                flags,
                sequence: 0,
                port: 0,
            },
            payload,
        }
    }

    /// Encode using the supplied request sequence and local port.
    pub fn encode(&self, sequence: u32, port: u32) -> Result<Vec<u8>> {
        let length = HEADER_LEN
            .checked_add(self.payload.len())
            .ok_or_else(|| Error::Encode("netlink message length overflow".into()))?;
        let length = u32::try_from(length)
            .map_err(|_| Error::Encode("netlink message exceeds u32 length".into()))?;
        let mut bytes = Vec::with_capacity(align(length as usize));
        bytes.extend_from_slice(
            &HeaderWire {
                length,
                message_type: self.header.message_type,
                flags: self.header.flags,
                sequence,
                port,
            }
            .to_bytes()
            .map_err(|error| Error::Encode(error.to_string()))?,
        );
        bytes.extend_from_slice(&self.payload);
        bytes.resize(align(bytes.len()), 0);
        Ok(bytes)
    }

    /// Decode every aligned message in one netlink datagram.
    pub fn decode_all(mut bytes: &[u8]) -> Result<Vec<Self>> {
        let mut messages = Vec::new();
        while !bytes.is_empty() {
            if bytes.len() < HEADER_LEN {
                return Err(Error::Decode(format!(
                    "truncated netlink header: {} bytes",
                    bytes.len()
                )));
            }
            let header = HeaderWire::decode_exact(&bytes[..HEADER_LEN])
                .map_err(|error| Error::Decode(error.to_string()))?;
            let length = header.length as usize;
            if length < HEADER_LEN {
                return Err(Error::Decode(format!(
                    "invalid netlink message length {length}"
                )));
            }
            if length > bytes.len() {
                return Err(Error::Decode(format!(
                    "netlink message length {length} exceeds datagram {}",
                    bytes.len()
                )));
            }
            messages.push(Self {
                header: Header {
                    length: header.length,
                    message_type: header.message_type,
                    flags: header.flags,
                    sequence: header.sequence,
                    port: header.port,
                },
                payload: bytes[HEADER_LEN..length].to_vec(),
            });
            let consumed = align(length);
            if consumed > bytes.len() {
                if length == bytes.len() {
                    break;
                }
                return Err(Error::Decode("truncated aligned netlink message".into()));
            }
            bytes = &bytes[consumed..];
        }
        Ok(messages)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// One owned `nlattr`, retaining unknown type and flag bits.
pub struct Attribute {
    /// Numeric type combined with `NLA_F_*` flag bits.
    pub kind: u16,
    /// Attribute payload without header or alignment padding.
    pub value: Vec<u8>,
}

impl Attribute {
    /// Construct an attribute from raw payload bytes.
    pub fn new(kind: u16, value: impl Into<Vec<u8>>) -> Self {
        Self {
            kind,
            value: value.into(),
        }
    }

    /// Construct an attribute containing an encoded nested list.
    pub fn nested(kind: u16, attributes: &[Self]) -> Result<Self> {
        Ok(Self::new(
            kind | NLA_F_NESTED,
            encode_attributes(attributes)?,
        ))
    }

    /// Construct a one-byte native value.
    pub fn u8(kind: u16, value: u8) -> Self {
        Self::new(kind, vec![value])
    }

    /// Construct a native-endian `u16` value.
    pub fn u16(kind: u16, value: u16) -> Self {
        Self::new(kind, value.to_ne_bytes().to_vec())
    }

    /// Construct a native-endian `u32` value.
    pub fn u32(kind: u16, value: u32) -> Self {
        Self::new(kind, value.to_ne_bytes().to_vec())
    }

    /// Construct a native-endian `i32` value.
    pub fn i32(kind: u16, value: i32) -> Self {
        Self::new(kind, value.to_ne_bytes().to_vec())
    }

    /// Construct a NUL-terminated string value.
    pub fn string(kind: u16, value: &str) -> Self {
        let mut bytes = value.as_bytes().to_vec();
        bytes.push(0);
        Self::new(kind, bytes)
    }

    /// Return the numeric type without flag bits.
    pub fn base_kind(&self) -> u16 {
        self.kind & NLA_TYPE_MASK
    }

    /// Whether the nested flag is set.
    pub fn is_nested(&self) -> bool {
        self.kind & NLA_F_NESTED != 0
    }

    /// Decode the payload as `u8`.
    pub fn as_u8(&self) -> Result<u8> {
        if self.value.len() != 1 {
            return Err(Error::Decode(format!(
                "attribute {} has {} bytes, expected one u8",
                self.base_kind(),
                self.value.len()
            )));
        }
        Ok(self.value[0])
    }

    /// Decode the payload as native-endian `u16`.
    pub fn as_u16(&self) -> Result<u16> {
        require_value_len(self, 2)?;
        read_u16(&self.value, 0)
    }

    /// Decode the payload as native-endian `u32`.
    pub fn as_u32(&self) -> Result<u32> {
        require_value_len(self, 4)?;
        read_u32(&self.value, 0)
    }

    /// Decode the payload as native-endian `i32`.
    pub fn as_i32(&self) -> Result<i32> {
        require_value_len(self, 4)?;
        Ok(i32::from_ne_bytes(take_array(&self.value, 0)?))
    }

    /// Decode a NUL-terminated or exact UTF-8 string.
    pub fn as_string(&self) -> Result<String> {
        let bytes = self
            .value
            .strip_suffix(&[0])
            .unwrap_or(self.value.as_slice());
        if bytes.contains(&0) {
            return Err(Error::Decode(format!(
                "attribute {} string contains an interior NUL",
                self.base_kind()
            )));
        }
        String::from_utf8(bytes.to_vec())
            .map_err(|error| Error::Decode(format!("attribute string is not UTF-8: {error}")))
    }

    /// Decode the payload as a nested attribute list.
    pub fn attributes(&self) -> Result<Vec<Self>> {
        decode_attributes(&self.value)
    }
}

/// Encode an aligned sequence of attributes.
pub fn encode_attributes(attributes: &[Attribute]) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    for attribute in attributes {
        let length = ATTRIBUTE_HEADER_LEN
            .checked_add(attribute.value.len())
            .ok_or_else(|| Error::Encode("attribute length overflow".into()))?;
        let length = u16::try_from(length)
            .map_err(|_| Error::Encode("attribute exceeds u16 length".into()))?;
        bytes.extend_from_slice(
            &AttributeHeaderWire {
                length,
                kind: attribute.kind,
            }
            .to_bytes()
            .map_err(|error| Error::Encode(error.to_string()))?,
        );
        bytes.extend_from_slice(&attribute.value);
        bytes.resize(align(bytes.len()), 0);
    }
    Ok(bytes)
}

/// Decode an aligned sequence of attributes, rejecting malformed lengths.
pub fn decode_attributes(mut bytes: &[u8]) -> Result<Vec<Attribute>> {
    let mut attributes = Vec::new();
    while !bytes.is_empty() {
        if bytes.len() < ATTRIBUTE_HEADER_LEN {
            return Err(Error::Decode("truncated attribute header".into()));
        }
        let header = AttributeHeaderWire::decode_exact(&bytes[..ATTRIBUTE_HEADER_LEN])
            .map_err(|error| Error::Decode(error.to_string()))?;
        let length = header.length as usize;
        let kind = header.kind;
        if length < ATTRIBUTE_HEADER_LEN || length > bytes.len() {
            return Err(Error::Decode(format!(
                "invalid attribute length {length} for {} remaining bytes",
                bytes.len()
            )));
        }
        attributes.push(Attribute {
            kind,
            value: bytes[ATTRIBUTE_HEADER_LEN..length].to_vec(),
        });
        let consumed = align(length);
        if consumed > bytes.len() {
            if length == bytes.len() {
                break;
            }
            return Err(Error::Decode("truncated aligned attribute".into()));
        }
        bytes = &bytes[consumed..];
    }
    Ok(attributes)
}

fn require_value_len(attribute: &Attribute, expected: usize) -> Result<()> {
    if attribute.value.len() == expected {
        Ok(())
    } else {
        Err(Error::Decode(format!(
            "attribute {} has {} bytes, expected {expected}",
            attribute.base_kind(),
            attribute.value.len()
        )))
    }
}

/// Round a byte count up to netlink alignment.
pub const fn align(length: usize) -> usize {
    (length + ALIGNMENT - 1) & !(ALIGNMENT - 1)
}

/// Append a native-endian `u16`.
pub fn put_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_ne_bytes());
}

/// Append a native-endian `u32`.
pub fn put_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_ne_bytes());
}

/// Append a native-endian `i32`.
pub fn put_i32(bytes: &mut Vec<u8>, value: i32) {
    bytes.extend_from_slice(&value.to_ne_bytes());
}

/// Read a native-endian `u16` at an offset.
pub fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_ne_bytes(take_array(bytes, offset)?))
}

/// Read a native-endian `u32` at an offset.
pub fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_ne_bytes(take_array(bytes, offset)?))
}

/// Read a native-endian `i32` at an offset.
pub fn read_i32(bytes: &[u8], offset: usize) -> Result<i32> {
    Ok(i32::from_ne_bytes(take_array(bytes, offset)?))
}

/// Copy a fixed-size array from an offset with bounds checking.
pub fn take_array<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N]> {
    let end = offset
        .checked_add(N)
        .ok_or_else(|| Error::Decode("offset overflow".into()))?;
    bytes
        .get(offset..end)
        .ok_or_else(|| {
            Error::Decode(format!(
                "need {N} bytes at offset {offset}, buffer has {}",
                bytes.len()
            ))
        })?
        .try_into()
        .map_err(|_| Error::Decode("array length mismatch".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_round_trip() {
        let original = Message::new(42, NLM_F_REQUEST | NLM_F_ACK, vec![1, 2, 3]);
        let encoded = original.encode(7, 9).unwrap();
        let decoded = Message::decode_all(&encoded).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].header.message_type, 42);
        assert_eq!(decoded[0].header.sequence, 7);
        assert_eq!(decoded[0].header.port, 9);
        assert_eq!(decoded[0].payload, vec![1, 2, 3]);
    }

    #[test]
    fn nested_attributes_round_trip() {
        let attributes = vec![
            Attribute::u32(1, 99),
            Attribute::string(2, "wg0"),
            Attribute::nested(3, &[Attribute::u8(1, 6)]).unwrap(),
        ];
        let decoded = decode_attributes(&encode_attributes(&attributes).unwrap()).unwrap();
        assert_eq!(decoded, attributes);
        assert_eq!(decoded[0].as_u32().unwrap(), 99);
        assert_eq!(decoded[1].as_string().unwrap(), "wg0");
        assert_eq!(decoded[2].attributes().unwrap()[0].as_u8().unwrap(), 6);
    }

    #[test]
    fn malformed_lengths_fail_closed() {
        assert!(Message::decode_all(&[8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).is_err());
        assert!(decode_attributes(&[3, 0, 1, 0]).is_err());
        assert!(decode_attributes(&[0, 0, 0, 0]).is_err());
        assert!(Attribute::new(1, vec![1, 2]).as_u8().is_err());
        assert!(
            Attribute::new(1, b"bad\0string\0".to_vec())
                .as_string()
                .is_err()
        );
    }
}
