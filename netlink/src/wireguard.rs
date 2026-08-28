//! `WireGuard` generic-netlink device state.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};

use zeroize::Zeroize;

use crate::core::{Attribute, Message, NLM_F_DUMP, NLM_F_REQUEST};
use crate::generic::Header;
use crate::{Error, Result};

#[cfg(feature = "tokio")]
use crate::generic::GenericClient;

const FAMILY_NAME: &str = "wireguard";
const VERSION: u8 = 1;
const CMD_GET_DEVICE: u8 = 0;
const CMD_SET_DEVICE: u8 = 1;

const DEVICE_IFINDEX: u16 = 1;
const DEVICE_IFNAME: u16 = 2;
const DEVICE_PRIVATE_KEY: u16 = 3;
const DEVICE_PUBLIC_KEY: u16 = 4;
const DEVICE_FLAGS: u16 = 5;
const DEVICE_LISTEN_PORT: u16 = 6;
const DEVICE_FWMARK: u16 = 7;
const DEVICE_PEERS: u16 = 8;

const DEVICE_REPLACE_PEERS: u32 = 1;

const PEER_PUBLIC_KEY: u16 = 1;
const PEER_PRESHARED_KEY: u16 = 2;
const PEER_FLAGS: u16 = 3;
const PEER_ENDPOINT: u16 = 4;
const PEER_KEEPALIVE: u16 = 5;
const PEER_LAST_HANDSHAKE: u16 = 6;
const PEER_RX_BYTES: u16 = 7;
const PEER_TX_BYTES: u16 = 8;
const PEER_ALLOWED_IPS: u16 = 9;
const PEER_PROTOCOL_VERSION: u16 = 10;

const PEER_REMOVE: u32 = 1;
const PEER_REPLACE_ALLOWED_IPS: u32 = 2;
const PEER_UPDATE_ONLY: u32 = 4;

const ALLOWED_IP_FAMILY: u16 = 1;
const ALLOWED_IP_ADDRESS: u16 = 2;
const ALLOWED_IP_CIDR: u16 = 3;

#[derive(Clone, PartialEq, Eq)]
/// A redacted, zeroized 32-byte `WireGuard` private or preshared key.
pub struct SecretKey([u8; 32]);

impl SecretKey {
    /// Wrap secret bytes without deriving or validating a public key.
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Explicitly expose secret bytes for a kernel request.
    pub fn expose_secret(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretKey([REDACTED])")
    }
}

impl Drop for SecretKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// A `WireGuard` public key.
pub struct PublicKey(pub [u8; 32]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// One peer allowed-IP prefix.
pub struct AllowedIp {
    /// IPv4 or IPv6 network address.
    pub address: IpAddr,
    /// CIDR prefix length.
    pub cidr: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Complete peer state returned by the kernel.
pub struct Peer {
    /// Peer public key and identity.
    pub public_key: PublicKey,
    /// Preshared key when the caller may read it and it is nonzero.
    pub preshared_key: Option<SecretKey>,
    /// Current IPv4 or IPv6 endpoint, including IPv6 scope.
    pub endpoint: Option<SocketAddr>,
    /// Persistent keepalive interval in seconds.
    pub persistent_keepalive: Option<u16>,
    /// Allowed source/destination prefixes.
    pub allowed_ips: Vec<AllowedIp>,
    /// Kernel `timespec64` `(seconds, nanoseconds)` for the last handshake.
    pub last_handshake: Option<(i64, i64)>,
    /// Received byte counter.
    pub received_bytes: u64,
    /// Transmitted byte counter.
    pub transmitted_bytes: u64,
    /// Optional protocol version reported by the kernel.
    pub protocol_version: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Complete `WireGuard` device state returned by the kernel.
pub struct Device {
    /// Interface index.
    pub index: u32,
    /// Interface name.
    pub name: String,
    /// Private key when readable and nonzero.
    pub private_key: Option<SecretKey>,
    /// Derived device public key.
    pub public_key: Option<PublicKey>,
    /// UDP listen port, or zero for automatic selection.
    pub listen_port: u16,
    /// Packet fwmark.
    pub fwmark: u32,
    /// Complete peer list, merged across multipart responses.
    pub peers: Vec<Peer>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Selective update for one peer.
pub struct PeerUpdate {
    /// Peer identity.
    pub public_key: PublicKey,
    /// New preshared key when supplied.
    pub preshared_key: Option<SecretKey>,
    /// New endpoint when supplied.
    pub endpoint: Option<SocketAddr>,
    /// New persistent keepalive when supplied.
    pub persistent_keepalive: Option<u16>,
    /// Allowed IPs supplied by this update.
    pub allowed_ips: Vec<AllowedIp>,
    /// Replace the peer's entire allowed-IP list.
    pub replace_allowed_ips: bool,
    /// Require the peer to exist rather than creating it.
    pub update_only: bool,
    /// Remove the peer.
    pub remove: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Selective update for a `WireGuard` device.
pub struct DeviceUpdate {
    /// Target interface name.
    pub name: String,
    /// New private key when supplied.
    pub private_key: Option<SecretKey>,
    /// New listen port when supplied.
    pub listen_port: Option<u16>,
    /// New fwmark when supplied.
    pub fwmark: Option<u32>,
    /// Replace all peers instead of updating the named peers.
    pub replace_peers: bool,
    /// Peer updates.
    pub peers: Vec<PeerUpdate>,
}

#[cfg(feature = "tokio")]
#[derive(Clone)]
/// Typed Tokio client for the kernel `WireGuard` generic-netlink family.
pub struct WireGuardClient {
    generic: GenericClient,
    family: u16,
}

#[cfg(feature = "tokio")]
impl WireGuardClient {
    /// Open generic netlink and resolve the `WireGuard` family.
    pub async fn open() -> Result<Self> {
        let generic = GenericClient::open()?;
        Self::from_generic(generic).await
    }

    /// Resolve the `WireGuard` family on an existing generic-netlink client.
    pub async fn from_generic(generic: GenericClient) -> Result<Self> {
        let family = generic.family_id(FAMILY_NAME).await?;
        Ok(Self { generic, family })
    }

    /// Read and merge complete device and peer state by interface name.
    pub async fn get_device(&self, name: &str) -> Result<Device> {
        let payload = Header {
            command: CMD_GET_DEVICE,
            version: VERSION,
        }
        .encode(&[Attribute::string(DEVICE_IFNAME, name)])?;
        let responses = self
            .generic
            .transport()
            .request(Message::new(
                self.family,
                NLM_F_REQUEST | NLM_F_DUMP,
                payload,
            ))
            .await?;
        let mut device = None;
        for response in responses {
            let (_, attributes) = Header::decode(&response.payload)?;
            merge_device(&mut device, &attributes)?;
        }
        device.ok_or_else(|| Error::Protocol(format!("WireGuard device {name:?} not found")))
    }

    /// Apply a selective update while preserving unspecified peers and fields.
    pub async fn set_device(&self, update: &DeviceUpdate) -> Result<()> {
        let mut attributes = vec![Attribute::string(DEVICE_IFNAME, &update.name)];
        if let Some(private_key) = &update.private_key {
            attributes.push(Attribute::new(
                DEVICE_PRIVATE_KEY,
                private_key.expose_secret().to_vec(),
            ));
        }
        if let Some(listen_port) = update.listen_port {
            attributes.push(Attribute::u16(DEVICE_LISTEN_PORT, listen_port));
        }
        if let Some(fwmark) = update.fwmark {
            attributes.push(Attribute::u32(DEVICE_FWMARK, fwmark));
        }
        if update.replace_peers {
            attributes.push(Attribute::u32(DEVICE_FLAGS, DEVICE_REPLACE_PEERS));
        }
        if !update.peers.is_empty() {
            let peers = update
                .peers
                .iter()
                .enumerate()
                .map(|(index, peer)| {
                    let kind = u16::try_from(index + 1)
                        .map_err(|_| Error::Encode("too many WireGuard peers".into()))?;
                    Attribute::nested(kind, &encode_peer_update(peer)?)
                })
                .collect::<Result<Vec<_>>>()?;
            attributes.push(Attribute::nested(DEVICE_PEERS, &peers)?);
        }
        let payload = Header {
            command: CMD_SET_DEVICE,
            version: VERSION,
        }
        .encode(&attributes)?;
        self.generic
            .transport()
            .request(Message::new(self.family, NLM_F_REQUEST, payload))
            .await?;
        Ok(())
    }
}

fn merge_device(device: &mut Option<Device>, attributes: &[Attribute]) -> Result<()> {
    if device.is_none() {
        let index = attribute(attributes, DEVICE_IFINDEX)
            .map(Attribute::as_u32)
            .transpose()?
            .unwrap_or(0);
        let name = attribute(attributes, DEVICE_IFNAME)
            .map(Attribute::as_string)
            .transpose()?
            .unwrap_or_default();
        *device = Some(Device {
            index,
            name,
            private_key: None,
            public_key: None,
            listen_port: 0,
            fwmark: 0,
            peers: Vec::new(),
        });
    }
    let device = device.as_mut().expect("initialized above");
    if let Some(attribute) = attribute(attributes, DEVICE_PRIVATE_KEY) {
        device.private_key = decode_secret(attribute)?;
    }
    if let Some(attribute) = attribute(attributes, DEVICE_PUBLIC_KEY) {
        device.public_key = decode_public(attribute)?;
    }
    if let Some(attribute) = attribute(attributes, DEVICE_LISTEN_PORT) {
        device.listen_port = attribute.as_u16()?;
    }
    if let Some(attribute) = attribute(attributes, DEVICE_FWMARK) {
        device.fwmark = attribute.as_u32()?;
    }
    if let Some(attribute) = attribute(attributes, DEVICE_PEERS) {
        for nested in attribute.attributes()? {
            merge_peer(&mut device.peers, decode_peer(&nested.attributes()?)?);
        }
    }
    Ok(())
}

fn merge_peer(peers: &mut Vec<Peer>, mut incoming: Peer) {
    let Some(existing) = peers
        .iter_mut()
        .find(|peer| peer.public_key == incoming.public_key)
    else {
        peers.push(incoming);
        return;
    };
    if incoming.preshared_key.is_some() {
        existing.preshared_key = incoming.preshared_key.take();
    }
    if incoming.endpoint.is_some() {
        existing.endpoint = incoming.endpoint;
    }
    if incoming.persistent_keepalive.is_some() {
        existing.persistent_keepalive = incoming.persistent_keepalive;
    }
    for allowed in incoming.allowed_ips {
        if !existing.allowed_ips.contains(&allowed) {
            existing.allowed_ips.push(allowed);
        }
    }
    if incoming.last_handshake.is_some() {
        existing.last_handshake = incoming.last_handshake;
    }
    existing.received_bytes = incoming.received_bytes;
    existing.transmitted_bytes = incoming.transmitted_bytes;
    if incoming.protocol_version.is_some() {
        existing.protocol_version = incoming.protocol_version;
    }
}

fn decode_peer(attributes: &[Attribute]) -> Result<Peer> {
    let public_key = decode_public(
        attribute(attributes, PEER_PUBLIC_KEY)
            .ok_or_else(|| Error::Decode("WireGuard peer omitted public key".into()))?,
    )?
    .ok_or_else(|| Error::Decode("WireGuard peer has an all-zero public key".into()))?;
    let allowed_ips = attribute(attributes, PEER_ALLOWED_IPS)
        .map(Attribute::attributes)
        .transpose()?
        .unwrap_or_default()
        .into_iter()
        .map(|nested| decode_allowed_ip(&nested.attributes()?))
        .collect::<Result<Vec<_>>>()?;
    Ok(Peer {
        public_key,
        preshared_key: attribute(attributes, PEER_PRESHARED_KEY)
            .map(decode_secret)
            .transpose()?
            .flatten(),
        endpoint: attribute(attributes, PEER_ENDPOINT)
            .map(|attribute| decode_endpoint(&attribute.value))
            .transpose()?,
        persistent_keepalive: attribute(attributes, PEER_KEEPALIVE)
            .map(Attribute::as_u16)
            .transpose()?,
        allowed_ips,
        last_handshake: attribute(attributes, PEER_LAST_HANDSHAKE)
            .map(|attribute| decode_timespec(&attribute.value))
            .transpose()?,
        received_bytes: attribute(attributes, PEER_RX_BYTES)
            .map(|attribute| decode_u64(&attribute.value))
            .transpose()?
            .unwrap_or(0),
        transmitted_bytes: attribute(attributes, PEER_TX_BYTES)
            .map(|attribute| decode_u64(&attribute.value))
            .transpose()?
            .unwrap_or(0),
        protocol_version: attribute(attributes, PEER_PROTOCOL_VERSION)
            .map(Attribute::as_u32)
            .transpose()?,
    })
}

fn decode_allowed_ip(attributes: &[Attribute]) -> Result<AllowedIp> {
    let family = attribute(attributes, ALLOWED_IP_FAMILY)
        .ok_or_else(|| Error::Decode("allowed IP omitted family".into()))?
        .as_u16()?;
    let address = attribute(attributes, ALLOWED_IP_ADDRESS)
        .ok_or_else(|| Error::Decode("allowed IP omitted address".into()))?;
    let address = match i32::from(family) {
        libc::AF_INET if address.value.len() == 4 => IpAddr::V4(Ipv4Addr::new(
            address.value[0],
            address.value[1],
            address.value[2],
            address.value[3],
        )),
        libc::AF_INET6 if address.value.len() == 16 => IpAddr::V6(Ipv6Addr::from(
            <[u8; 16]>::try_from(&address.value[..])
                .map_err(|_| Error::Decode("invalid allowed IPv6 address".into()))?,
        )),
        _ => {
            return Err(Error::Decode(format!(
                "invalid allowed IP family {family} and length {}",
                address.value.len()
            )));
        }
    };
    let cidr = attribute(attributes, ALLOWED_IP_CIDR)
        .ok_or_else(|| Error::Decode("allowed IP omitted CIDR".into()))?
        .as_u8()?;
    let maximum = if address.is_ipv4() { 32 } else { 128 };
    if cidr > maximum {
        return Err(Error::Decode(format!(
            "invalid allowed-IP prefix length {cidr} for {address}"
        )));
    }
    Ok(AllowedIp { address, cidr })
}

fn encode_peer_update(peer: &PeerUpdate) -> Result<Vec<Attribute>> {
    let mut attributes = vec![Attribute::new(PEER_PUBLIC_KEY, peer.public_key.0.to_vec())];
    if let Some(preshared) = &peer.preshared_key {
        attributes.push(Attribute::new(
            PEER_PRESHARED_KEY,
            preshared.expose_secret().to_vec(),
        ));
    }
    let flags = (u32::from(peer.remove) * PEER_REMOVE)
        | (u32::from(peer.replace_allowed_ips) * PEER_REPLACE_ALLOWED_IPS)
        | (u32::from(peer.update_only) * PEER_UPDATE_ONLY);
    if flags != 0 {
        attributes.push(Attribute::u32(PEER_FLAGS, flags));
    }
    if let Some(endpoint) = peer.endpoint {
        attributes.push(Attribute::new(PEER_ENDPOINT, encode_endpoint(endpoint)));
    }
    if let Some(keepalive) = peer.persistent_keepalive {
        attributes.push(Attribute::u16(PEER_KEEPALIVE, keepalive));
    }
    if !peer.allowed_ips.is_empty() || peer.replace_allowed_ips {
        let allowed_ips = peer
            .allowed_ips
            .iter()
            .enumerate()
            .map(|(index, allowed)| {
                let kind = u16::try_from(index + 1)
                    .map_err(|_| Error::Encode("too many allowed IPs".into()))?;
                Attribute::nested(kind, &encode_allowed_ip(*allowed))
            })
            .collect::<Result<Vec<_>>>()?;
        attributes.push(Attribute::nested(PEER_ALLOWED_IPS, &allowed_ips)?);
    }
    Ok(attributes)
}

fn encode_allowed_ip(allowed: AllowedIp) -> Vec<Attribute> {
    let (family, bytes) = match allowed.address {
        IpAddr::V4(address) => (libc::AF_INET as u16, address.octets().to_vec()),
        IpAddr::V6(address) => (libc::AF_INET6 as u16, address.octets().to_vec()),
    };
    vec![
        Attribute::u16(ALLOWED_IP_FAMILY, family),
        Attribute::new(ALLOWED_IP_ADDRESS, bytes),
        Attribute::u8(ALLOWED_IP_CIDR, allowed.cidr),
    ]
}

fn attribute(attributes: &[Attribute], kind: u16) -> Option<&Attribute> {
    attributes
        .iter()
        .find(|attribute| attribute.base_kind() == kind)
}

fn decode_secret(attribute: &Attribute) -> Result<Option<SecretKey>> {
    let bytes = <[u8; 32]>::try_from(attribute.value.as_slice())
        .map_err(|_| Error::Decode("WireGuard secret key is not 32 bytes".into()))?;
    Ok((bytes != [0; 32]).then(|| SecretKey::new(bytes)))
}

fn decode_public(attribute: &Attribute) -> Result<Option<PublicKey>> {
    let bytes = <[u8; 32]>::try_from(attribute.value.as_slice())
        .map_err(|_| Error::Decode("WireGuard public key is not 32 bytes".into()))?;
    Ok((bytes != [0; 32]).then_some(PublicKey(bytes)))
}

fn decode_timespec(bytes: &[u8]) -> Result<(i64, i64)> {
    if bytes.len() != 16 {
        return Err(Error::Decode("WireGuard timespec is not 16 bytes".into()));
    }
    Ok((
        i64::from_ne_bytes(bytes[0..8].try_into().expect("length checked")),
        i64::from_ne_bytes(bytes[8..16].try_into().expect("length checked")),
    ))
}

fn decode_u64(bytes: &[u8]) -> Result<u64> {
    Ok(u64::from_ne_bytes(bytes.try_into().map_err(|_| {
        Error::Decode("expected u64 attribute".into())
    })?))
}

fn encode_endpoint(endpoint: SocketAddr) -> Vec<u8> {
    match endpoint {
        SocketAddr::V4(endpoint) => {
            let mut bytes = Vec::with_capacity(16);
            bytes.extend_from_slice(&(libc::AF_INET as u16).to_ne_bytes());
            bytes.extend_from_slice(&endpoint.port().to_be_bytes());
            bytes.extend_from_slice(&endpoint.ip().octets());
            bytes.extend_from_slice(&[0; 8]);
            bytes
        }
        SocketAddr::V6(endpoint) => {
            let mut bytes = Vec::with_capacity(28);
            bytes.extend_from_slice(&(libc::AF_INET6 as u16).to_ne_bytes());
            bytes.extend_from_slice(&endpoint.port().to_be_bytes());
            bytes.extend_from_slice(&endpoint.flowinfo().to_be_bytes());
            bytes.extend_from_slice(&endpoint.ip().octets());
            bytes.extend_from_slice(&endpoint.scope_id().to_ne_bytes());
            bytes
        }
    }
}

fn decode_endpoint(bytes: &[u8]) -> Result<SocketAddr> {
    if bytes.len() < 2 {
        return Err(Error::Decode("truncated WireGuard endpoint".into()));
    }
    let family = u16::from_ne_bytes(bytes[0..2].try_into().expect("length checked"));
    match i32::from(family) {
        libc::AF_INET if bytes.len() == 16 => Ok(SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(bytes[4], bytes[5], bytes[6], bytes[7]),
            u16::from_be_bytes(bytes[2..4].try_into().expect("length checked")),
        ))),
        libc::AF_INET6 if bytes.len() == 28 => Ok(SocketAddr::V6(SocketAddrV6::new(
            Ipv6Addr::from(<[u8; 16]>::try_from(&bytes[8..24]).expect("length checked")),
            u16::from_be_bytes(bytes[2..4].try_into().expect("length checked")),
            u32::from_be_bytes(bytes[4..8].try_into().expect("length checked")),
            u32::from_ne_bytes(bytes[24..28].try_into().expect("length checked")),
        ))),
        _ => Err(Error::Decode(format!(
            "unsupported WireGuard endpoint family {family} with {} bytes",
            bytes.len()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_redacted() {
        let key = SecretKey::new([7; 32]);
        assert_eq!(format!("{key:?}"), "SecretKey([REDACTED])");
        assert!(!format!("{key:?}").contains('7'));
    }

    #[test]
    fn endpoints_round_trip_with_ipv6_scope() {
        let endpoints = [
            "192.0.2.1:51820".parse().unwrap(),
            SocketAddr::V6(SocketAddrV6::new("fe80::1".parse().unwrap(), 51820, 4, 9)),
        ];
        for endpoint in endpoints {
            assert_eq!(
                decode_endpoint(&encode_endpoint(endpoint)).unwrap(),
                endpoint
            );
        }
    }

    #[test]
    fn peer_updates_do_not_replace_by_default() {
        let update = PeerUpdate {
            public_key: PublicKey([1; 32]),
            preshared_key: None,
            endpoint: None,
            persistent_keepalive: None,
            allowed_ips: Vec::new(),
            replace_allowed_ips: false,
            update_only: true,
            remove: false,
        };
        let attributes = encode_peer_update(&update).unwrap();
        assert_eq!(
            attribute(&attributes, PEER_FLAGS)
                .unwrap()
                .as_u32()
                .unwrap(),
            PEER_UPDATE_ONLY
        );
        assert!(attribute(&attributes, PEER_ALLOWED_IPS).is_none());
    }

    #[test]
    fn multipart_peer_fragments_merge_by_public_key() {
        let key = PublicKey([4; 32]);
        let mut peers = vec![Peer {
            public_key: key,
            preshared_key: None,
            endpoint: None,
            persistent_keepalive: None,
            allowed_ips: vec![AllowedIp {
                address: "10.0.0.0".parse().unwrap(),
                cidr: 8,
            }],
            last_handshake: None,
            received_bytes: 1,
            transmitted_bytes: 2,
            protocol_version: None,
        }];
        merge_peer(
            &mut peers,
            Peer {
                public_key: key,
                preshared_key: None,
                endpoint: Some("192.0.2.1:51820".parse().unwrap()),
                persistent_keepalive: Some(25),
                allowed_ips: vec![AllowedIp {
                    address: "2001:db8::".parse().unwrap(),
                    cidr: 32,
                }],
                last_handshake: Some((10, 20)),
                received_bytes: 3,
                transmitted_bytes: 4,
                protocol_version: Some(1),
            },
        );
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].allowed_ips.len(), 2);
        assert_eq!(peers[0].persistent_keepalive, Some(25));
        assert_eq!(peers[0].received_bytes, 3);
    }
}
