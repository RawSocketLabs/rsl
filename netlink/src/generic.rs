//! Generic-netlink controller helpers.

use crate::core::{Attribute, Message, NLM_F_REQUEST, decode_attributes, encode_attributes};
use crate::{Error, Result};

use crate::transport::{Client, Protocol};

const GENL_ID_CTRL: u16 = 0x10;
const CTRL_CMD_GETFAMILY: u8 = 3;
const CTRL_ATTR_FAMILY_ID: u16 = 1;
const CTRL_ATTR_FAMILY_NAME: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Header {
    pub(crate) command: u8,
    pub(crate) version: u8,
}

impl Header {
    pub(crate) fn encode(self, attributes: &[Attribute]) -> Result<Vec<u8>> {
        let mut payload = vec![self.command, self.version, 0, 0];
        payload.extend_from_slice(&encode_attributes(attributes)?);
        Ok(payload)
    }

    pub(crate) fn decode(payload: &[u8]) -> Result<(Self, Vec<Attribute>)> {
        if payload.len() < 4 {
            return Err(Error::Decode("truncated generic-netlink header".into()));
        }
        Ok((
            Self {
                command: payload[0],
                version: payload[1],
            },
            decode_attributes(&payload[4..])?,
        ))
    }
}

#[derive(Clone)]
/// Generic-netlink client and controller-family resolver.
pub struct GenericClient {
    transport: Client,
}

impl GenericClient {
    /// Open a nonblocking `NETLINK_GENERIC` socket.
    pub fn open() -> Result<Self> {
        Ok(Self {
            transport: Client::open(Protocol::Generic)?,
        })
    }

    /// Wrap an existing generic-netlink transport.
    pub fn from_transport(transport: Client) -> Self {
        Self { transport }
    }

    /// Borrow the underlying request transport.
    pub fn transport(&self) -> &Client {
        &self.transport
    }

    /// Resolve a generic-netlink family name to its kernel-assigned ID.
    pub fn family_id(&self, name: &str) -> Result<u16> {
        let payload = Header {
            command: CTRL_CMD_GETFAMILY,
            version: 1,
        }
        .encode(&[Attribute::string(CTRL_ATTR_FAMILY_NAME, name)])?;
        let responses =
            self.transport
                .request(Message::new(GENL_ID_CTRL, NLM_F_REQUEST, payload))?;
        for response in responses {
            let (_, attributes) = Header::decode(&response.payload)?;
            if let Some(attribute) = attributes
                .iter()
                .find(|attribute| attribute.base_kind() == CTRL_ATTR_FAMILY_ID)
            {
                return attribute.as_u16();
            }
        }
        Err(Error::Unsupported(format!(
            "generic-netlink family {name:?} is unavailable"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_controller_family() {
        let client = GenericClient::open().unwrap();
        assert_eq!(client.family_id("nlctrl").unwrap(), GENL_ID_CTRL);
    }
}
