//! Typed route-netlink inspection and mutation.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::os::fd::{AsRawFd, BorrowedFd};

use crate::core::{
    Attribute, Message, NLM_F_CREATE, NLM_F_DUMP, NLM_F_EXCL, NLM_F_REPLACE, NLM_F_REQUEST,
    decode_attributes, encode_attributes, put_i32, put_u16, put_u32, read_i32, read_u32,
};
use crate::{Error, Result};

#[cfg(feature = "tokio")]
use crate::transport::{Client, Protocol};

const RTM_NEWLINK: u16 = 16;
const RTM_DELLINK: u16 = 17;
const RTM_GETLINK: u16 = 18;
const RTM_SETLINK: u16 = 19;
const RTM_NEWADDR: u16 = 20;
const RTM_DELADDR: u16 = 21;
const RTM_GETADDR: u16 = 22;
const RTM_NEWROUTE: u16 = 24;
const RTM_DELROUTE: u16 = 25;
const RTM_GETROUTE: u16 = 26;
const RTM_NEWRULE: u16 = 32;
const RTM_DELRULE: u16 = 33;
const RTM_GETRULE: u16 = 34;

const IFLA_IFNAME: u16 = 3;
const IFLA_MTU: u16 = 4;
const IFLA_LINKINFO: u16 = 18;
const IFLA_NET_NS_FD: u16 = 28;
const IFLA_INFO_KIND: u16 = 1;

const IFA_ADDRESS: u16 = 1;
const IFA_LOCAL: u16 = 2;

const RTA_DST: u16 = 1;
const RTA_SRC: u16 = 2;
const RTA_OIF: u16 = 4;
const RTA_GATEWAY: u16 = 5;
const RTA_PRIORITY: u16 = 6;
const RTA_TABLE: u16 = 15;

const FRA_DST: u16 = 1;
const FRA_SRC: u16 = 2;
const FRA_PRIORITY: u16 = 6;
const FRA_FWMARK: u16 = 10;
const FRA_TABLE: u16 = 15;
const FRA_FWMASK: u16 = 16;

const IFF_UP: u32 = 1;
const RT_TABLE_UNSPEC: u8 = 0;
const RTPROT_BOOT: u8 = 3;
const RT_SCOPE_UNIVERSE: u8 = 0;
const RTN_UNICAST: u8 = 1;
const FR_ACT_TO_TBL: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Kernel link state needed for migration.
pub struct Link {
    /// Interface index in the socket's network namespace.
    pub index: u32,
    /// Kernel interface name.
    pub name: String,
    /// Configured MTU when reported.
    pub mtu: Option<u32>,
    /// Raw `IFF_*` flags.
    pub flags: u32,
    /// Kernel link kind, such as `wireguard`, when reported.
    pub kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Address assigned to an interface.
pub struct Address {
    /// Owning interface index.
    pub interface: u32,
    /// Prefix or peer address from `IFA_ADDRESS`.
    pub address: IpAddr,
    /// Local address for point-to-point configurations.
    pub local: Option<IpAddr>,
    /// CIDR prefix length.
    pub prefix_len: u8,
    /// Raw address flags.
    pub flags: u8,
    /// Kernel address scope.
    pub scope: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// IPv4 or IPv6 route suitable for snapshot and restoration.
pub struct Route {
    /// Linux address-family number.
    pub family: u8,
    /// Optional destination; `None` denotes the default prefix.
    pub destination: Option<IpAddr>,
    /// Destination CIDR length.
    pub destination_prefix: u8,
    /// Optional source selector.
    pub source: Option<IpAddr>,
    /// Source CIDR length.
    pub source_prefix: u8,
    /// Optional next-hop address.
    pub gateway: Option<IpAddr>,
    /// Optional output interface index.
    pub output_interface: Option<u32>,
    /// Routing-table number.
    pub table: u32,
    /// Optional route priority/metric.
    pub priority: Option<u32>,
    /// Kernel route protocol.
    pub protocol: u8,
    /// Kernel route scope.
    pub scope: u8,
    /// Kernel route type.
    pub route_type: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Policy-routing rule suitable for snapshot and restoration.
pub struct Rule {
    /// Linux address-family number.
    pub family: u8,
    /// Optional destination selector.
    pub destination: Option<IpAddr>,
    /// Destination CIDR length.
    pub destination_prefix: u8,
    /// Optional source selector.
    pub source: Option<IpAddr>,
    /// Source CIDR length.
    pub source_prefix: u8,
    /// Target routing-table number.
    pub table: u32,
    /// Optional rule priority.
    pub priority: Option<u32>,
    /// Optional packet-mark selector.
    pub fwmark: Option<u32>,
    /// Optional mark mask.
    pub fwmask: Option<u32>,
    /// Kernel rule action.
    pub action: u8,
}

#[cfg(feature = "tokio")]
#[derive(Clone)]
/// Typed Tokio client for `NETLINK_ROUTE`.
pub struct RouteClient {
    transport: Client,
}

#[cfg(feature = "tokio")]
impl RouteClient {
    /// Open a route-netlink socket in the caller's current network namespace.
    pub fn open() -> Result<Self> {
        Ok(Self {
            transport: Client::open(Protocol::Route)?,
        })
    }

    /// Wrap an existing route-netlink transport.
    pub fn from_transport(transport: Client) -> Self {
        Self { transport }
    }

    /// Dump all links visible to the socket.
    pub async fn links(&self) -> Result<Vec<Link>> {
        let responses = self
            .transport
            .request(Message::new(
                RTM_GETLINK,
                NLM_F_REQUEST | NLM_F_DUMP,
                ifinfo_payload(0, 0, 0, &[])?,
            ))
            .await?;
        responses
            .into_iter()
            .filter(|message| message.header.message_type == RTM_NEWLINK)
            .map(|message| decode_link(&message.payload))
            .collect()
    }

    /// Find a link by its kernel name.
    pub async fn link_by_name(&self, name: &str) -> Result<Link> {
        self.links()
            .await?
            .into_iter()
            .find(|link| link.name == name)
            .ok_or_else(|| Error::Protocol(format!("link {name:?} not found")))
    }

    /// Dump all addresses visible to the socket.
    pub async fn addresses(&self) -> Result<Vec<Address>> {
        let responses = self
            .transport
            .request(Message::new(
                RTM_GETADDR,
                NLM_F_REQUEST | NLM_F_DUMP,
                ifaddr_payload(0, 0, 0, 0, 0, &[])?,
            ))
            .await?;
        responses
            .into_iter()
            .filter(|message| message.header.message_type == RTM_NEWADDR)
            .map(|message| decode_address(&message.payload))
            .collect()
    }

    /// Dump all IPv4 and IPv6 routes visible to the socket.
    pub async fn routes(&self) -> Result<Vec<Route>> {
        let responses = self
            .transport
            .request(Message::new(
                RTM_GETROUTE,
                NLM_F_REQUEST | NLM_F_DUMP,
                route_payload(0, 0, 0, 0, 0, 0, 0, 0, 0, &[])?,
            ))
            .await?;
        responses
            .into_iter()
            .filter(|message| message.header.message_type == RTM_NEWROUTE)
            .map(|message| decode_route(&message.payload))
            .collect()
    }

    /// Dump all IPv4 and IPv6 policy rules visible to the socket.
    pub async fn rules(&self) -> Result<Vec<Rule>> {
        let responses = self
            .transport
            .request(Message::new(
                RTM_GETRULE,
                NLM_F_REQUEST | NLM_F_DUMP,
                rule_payload(0, 0, 0, 0, 0, 0, &[])?,
            ))
            .await?;
        responses
            .into_iter()
            .filter(|message| message.header.message_type == RTM_NEWRULE)
            .map(|message| decode_rule(&message.payload))
            .collect()
    }

    /// Move an interface to the namespace identified by an open fd.
    pub async fn set_link_namespace(&self, index: u32, namespace: BorrowedFd<'_>) -> Result<()> {
        let raw_fd = namespace.as_raw_fd();
        self.set_link(index, 0, 0, &[Attribute::i32(IFLA_NET_NS_FD, raw_fd)])
            .await
    }

    /// Set or clear `IFF_UP` without disturbing other flags.
    pub async fn set_link_up(&self, index: u32, up: bool) -> Result<()> {
        self.set_link(index, if up { IFF_UP } else { 0 }, IFF_UP, &[])
            .await
    }

    /// Set an interface MTU.
    pub async fn set_mtu(&self, index: u32, mtu: u32) -> Result<()> {
        self.set_link(index, 0, 0, &[Attribute::u32(IFLA_MTU, mtu)])
            .await
    }

    async fn set_link(
        &self,
        index: u32,
        flags: u32,
        change: u32,
        attributes: &[Attribute],
    ) -> Result<()> {
        self.transport
            .request(Message::new(
                RTM_SETLINK,
                NLM_F_REQUEST,
                ifinfo_payload(index, flags, change, attributes)?,
            ))
            .await?;
        Ok(())
    }

    /// Create a WireGuard-kind link.
    pub async fn create_wireguard(&self, name: &str) -> Result<()> {
        self.create_link(name, "wireguard").await
    }

    /// Create a link with a kernel `IFLA_INFO_KIND`, such as `dummy`.
    pub async fn create_link(&self, name: &str, kind: &str) -> Result<()> {
        let link_info =
            Attribute::nested(IFLA_LINKINFO, &[Attribute::string(IFLA_INFO_KIND, kind)])?;
        self.transport
            .request(Message::new(
                RTM_NEWLINK,
                NLM_F_REQUEST | NLM_F_CREATE | NLM_F_EXCL,
                ifinfo_payload(0, 0, 0, &[Attribute::string(IFLA_IFNAME, name), link_info])?,
            ))
            .await?;
        Ok(())
    }

    /// Delete a link by index.
    pub async fn delete_link(&self, index: u32) -> Result<()> {
        self.transport
            .request(Message::new(
                RTM_DELLINK,
                NLM_F_REQUEST,
                ifinfo_payload(index, 0, 0, &[])?,
            ))
            .await?;
        Ok(())
    }

    /// Add or replace an address.
    pub async fn add_address(&self, address: &Address) -> Result<()> {
        let mut attributes = vec![Attribute::new(IFA_ADDRESS, ip_bytes(address.address))];
        if let Some(local) = address.local {
            attributes.push(Attribute::new(IFA_LOCAL, ip_bytes(local)));
        }
        self.transport
            .request(Message::new(
                RTM_NEWADDR,
                NLM_F_REQUEST | NLM_F_CREATE | NLM_F_REPLACE,
                ifaddr_payload(
                    address_family(address.address),
                    address.prefix_len,
                    address.flags,
                    address.scope,
                    address.interface,
                    &attributes,
                )?,
            ))
            .await?;
        Ok(())
    }

    /// Delete an exact address assignment.
    pub async fn delete_address(&self, address: &Address) -> Result<()> {
        let mut attributes = vec![Attribute::new(IFA_ADDRESS, ip_bytes(address.address))];
        if let Some(local) = address.local {
            attributes.push(Attribute::new(IFA_LOCAL, ip_bytes(local)));
        }
        self.transport
            .request(Message::new(
                RTM_DELADDR,
                NLM_F_REQUEST,
                ifaddr_payload(
                    address_family(address.address),
                    address.prefix_len,
                    address.flags,
                    address.scope,
                    address.interface,
                    &attributes,
                )?,
            ))
            .await?;
        Ok(())
    }

    /// Add or replace a route.
    pub async fn add_route(&self, route: &Route) -> Result<()> {
        let mut attributes = Vec::new();
        if let Some(destination) = route.destination {
            attributes.push(Attribute::new(RTA_DST, ip_bytes(destination)));
        }
        if let Some(source) = route.source {
            attributes.push(Attribute::new(RTA_SRC, ip_bytes(source)));
        }
        if let Some(gateway) = route.gateway {
            attributes.push(Attribute::new(RTA_GATEWAY, ip_bytes(gateway)));
        }
        if let Some(interface) = route.output_interface {
            attributes.push(Attribute::u32(RTA_OIF, interface));
        }
        if let Some(priority) = route.priority {
            attributes.push(Attribute::u32(RTA_PRIORITY, priority));
        }
        let table = if route.table <= u8::MAX.into() {
            route.table as u8
        } else {
            attributes.push(Attribute::u32(RTA_TABLE, route.table));
            RT_TABLE_UNSPEC
        };
        self.transport
            .request(Message::new(
                RTM_NEWROUTE,
                NLM_F_REQUEST | NLM_F_CREATE | NLM_F_REPLACE,
                route_payload(
                    route.family,
                    route.destination_prefix,
                    route.source_prefix,
                    table,
                    route.protocol,
                    route.scope,
                    route.route_type,
                    0,
                    0,
                    &attributes,
                )?,
            ))
            .await?;
        Ok(())
    }

    /// Delete an exact route.
    pub async fn delete_route(&self, route: &Route) -> Result<()> {
        let (table, attributes) = encode_route_attributes(route);
        self.transport
            .request(Message::new(
                RTM_DELROUTE,
                NLM_F_REQUEST,
                route_payload(
                    route.family,
                    route.destination_prefix,
                    route.source_prefix,
                    table,
                    route.protocol,
                    route.scope,
                    route.route_type,
                    0,
                    0,
                    &attributes,
                )?,
            ))
            .await?;
        Ok(())
    }

    /// Add or replace a policy rule.
    pub async fn add_rule(&self, rule: &Rule) -> Result<()> {
        let mut attributes = Vec::new();
        if let Some(destination) = rule.destination {
            attributes.push(Attribute::new(FRA_DST, ip_bytes(destination)));
        }
        if let Some(source) = rule.source {
            attributes.push(Attribute::new(FRA_SRC, ip_bytes(source)));
        }
        if let Some(priority) = rule.priority {
            attributes.push(Attribute::u32(FRA_PRIORITY, priority));
        }
        if let Some(mark) = rule.fwmark {
            attributes.push(Attribute::u32(FRA_FWMARK, mark));
        }
        if let Some(mask) = rule.fwmask {
            attributes.push(Attribute::u32(FRA_FWMASK, mask));
        }
        let table = if rule.table <= u8::MAX.into() {
            rule.table as u8
        } else {
            attributes.push(Attribute::u32(FRA_TABLE, rule.table));
            RT_TABLE_UNSPEC
        };
        self.transport
            .request(Message::new(
                RTM_NEWRULE,
                NLM_F_REQUEST | NLM_F_CREATE | NLM_F_REPLACE,
                rule_payload(
                    rule.family,
                    rule.destination_prefix,
                    rule.source_prefix,
                    table,
                    rule.action,
                    0,
                    &attributes,
                )?,
            ))
            .await?;
        Ok(())
    }

    /// Delete an exact policy-routing rule.
    pub async fn delete_rule(&self, rule: &Rule) -> Result<()> {
        let (table, attributes) = encode_rule_attributes(rule);
        self.transport
            .request(Message::new(
                RTM_DELRULE,
                NLM_F_REQUEST,
                rule_payload(
                    rule.family,
                    rule.destination_prefix,
                    rule.source_prefix,
                    table,
                    rule.action,
                    0,
                    &attributes,
                )?,
            ))
            .await?;
        Ok(())
    }
}

fn encode_route_attributes(route: &Route) -> (u8, Vec<Attribute>) {
    let mut attributes = Vec::new();
    if let Some(destination) = route.destination {
        attributes.push(Attribute::new(RTA_DST, ip_bytes(destination)));
    }
    if let Some(source) = route.source {
        attributes.push(Attribute::new(RTA_SRC, ip_bytes(source)));
    }
    if let Some(gateway) = route.gateway {
        attributes.push(Attribute::new(RTA_GATEWAY, ip_bytes(gateway)));
    }
    if let Some(interface) = route.output_interface {
        attributes.push(Attribute::u32(RTA_OIF, interface));
    }
    if let Some(priority) = route.priority {
        attributes.push(Attribute::u32(RTA_PRIORITY, priority));
    }
    let table = if route.table <= u8::MAX.into() {
        route.table as u8
    } else {
        attributes.push(Attribute::u32(RTA_TABLE, route.table));
        RT_TABLE_UNSPEC
    };
    (table, attributes)
}

fn encode_rule_attributes(rule: &Rule) -> (u8, Vec<Attribute>) {
    let mut attributes = Vec::new();
    if let Some(destination) = rule.destination {
        attributes.push(Attribute::new(FRA_DST, ip_bytes(destination)));
    }
    if let Some(source) = rule.source {
        attributes.push(Attribute::new(FRA_SRC, ip_bytes(source)));
    }
    if let Some(priority) = rule.priority {
        attributes.push(Attribute::u32(FRA_PRIORITY, priority));
    }
    if let Some(mark) = rule.fwmark {
        attributes.push(Attribute::u32(FRA_FWMARK, mark));
    }
    if let Some(mask) = rule.fwmask {
        attributes.push(Attribute::u32(FRA_FWMASK, mask));
    }
    let table = if rule.table <= u8::MAX.into() {
        rule.table as u8
    } else {
        attributes.push(Attribute::u32(FRA_TABLE, rule.table));
        RT_TABLE_UNSPEC
    };
    (table, attributes)
}

impl Default for Route {
    fn default() -> Self {
        Self {
            family: libc::AF_INET as u8,
            destination: None,
            destination_prefix: 0,
            source: None,
            source_prefix: 0,
            gateway: None,
            output_interface: None,
            table: u32::from(libc::RT_TABLE_MAIN),
            priority: None,
            protocol: RTPROT_BOOT,
            scope: RT_SCOPE_UNIVERSE,
            route_type: RTN_UNICAST,
        }
    }
}

impl Default for Rule {
    fn default() -> Self {
        Self {
            family: libc::AF_INET as u8,
            destination: None,
            destination_prefix: 0,
            source: None,
            source_prefix: 0,
            table: u32::from(libc::RT_TABLE_MAIN),
            priority: None,
            fwmark: None,
            fwmask: None,
            action: FR_ACT_TO_TBL,
        }
    }
}

fn ifinfo_payload(
    index: u32,
    flags: u32,
    change: u32,
    attributes: &[Attribute],
) -> Result<Vec<u8>> {
    let mut payload = vec![libc::AF_UNSPEC as u8, 0];
    put_u16(&mut payload, 0);
    put_i32(
        &mut payload,
        i32::try_from(index).map_err(|_| Error::Encode("link index out of range".into()))?,
    );
    put_u32(&mut payload, flags);
    put_u32(&mut payload, change);
    payload.extend_from_slice(&encode_attributes(attributes)?);
    Ok(payload)
}

fn ifaddr_payload(
    family: u8,
    prefix: u8,
    flags: u8,
    scope: u8,
    index: u32,
    attributes: &[Attribute],
) -> Result<Vec<u8>> {
    let mut payload = vec![family, prefix, flags, scope];
    put_u32(&mut payload, index);
    payload.extend_from_slice(&encode_attributes(attributes)?);
    Ok(payload)
}

#[allow(clippy::too_many_arguments)]
fn route_payload(
    family: u8,
    destination_prefix: u8,
    source_prefix: u8,
    table: u8,
    protocol: u8,
    scope: u8,
    route_type: u8,
    tos: u8,
    flags: u32,
    attributes: &[Attribute],
) -> Result<Vec<u8>> {
    let mut payload = vec![
        family,
        destination_prefix,
        source_prefix,
        tos,
        table,
        protocol,
        scope,
        route_type,
    ];
    put_u32(&mut payload, flags);
    payload.extend_from_slice(&encode_attributes(attributes)?);
    Ok(payload)
}

#[allow(clippy::too_many_arguments)]
fn rule_payload(
    family: u8,
    destination_prefix: u8,
    source_prefix: u8,
    table: u8,
    action: u8,
    flags: u32,
    attributes: &[Attribute],
) -> Result<Vec<u8>> {
    let mut payload = vec![
        family,
        destination_prefix,
        source_prefix,
        0,
        table,
        0,
        0,
        action,
    ];
    put_u32(&mut payload, flags);
    payload.extend_from_slice(&encode_attributes(attributes)?);
    Ok(payload)
}

fn decode_link(payload: &[u8]) -> Result<Link> {
    if payload.len() < 16 {
        return Err(Error::Decode("truncated ifinfomsg".into()));
    }
    let index = u32::try_from(read_i32(payload, 4)?)
        .map_err(|_| Error::Decode("negative link index".into()))?;
    let flags = read_u32(payload, 8)?;
    let attributes = decode_attributes(&payload[16..])?;
    let name = attributes
        .iter()
        .find(|attribute| attribute.base_kind() == IFLA_IFNAME)
        .ok_or_else(|| Error::Decode("link response omitted IFLA_IFNAME".into()))?
        .as_string()?;
    let mtu = attributes
        .iter()
        .find(|attribute| attribute.base_kind() == IFLA_MTU)
        .map(Attribute::as_u32)
        .transpose()?;
    let kind = attributes
        .iter()
        .find(|attribute| attribute.base_kind() == IFLA_LINKINFO)
        .map(Attribute::attributes)
        .transpose()?
        .and_then(|attributes| {
            attributes
                .into_iter()
                .find(|attribute| attribute.base_kind() == IFLA_INFO_KIND)
        })
        .map(|attribute| attribute.as_string())
        .transpose()?;
    Ok(Link {
        index,
        name,
        mtu,
        flags,
        kind,
    })
}

fn decode_address(payload: &[u8]) -> Result<Address> {
    if payload.len() < 8 {
        return Err(Error::Decode("truncated ifaddrmsg".into()));
    }
    let family = payload[0];
    let attributes = decode_attributes(&payload[8..])?;
    let address = attributes
        .iter()
        .find(|attribute| attribute.base_kind() == IFA_ADDRESS)
        .ok_or_else(|| Error::Decode("address response omitted IFA_ADDRESS".into()))?;
    let local = attributes
        .iter()
        .find(|attribute| attribute.base_kind() == IFA_LOCAL)
        .map(|attribute| decode_ip(family, &attribute.value))
        .transpose()?;
    Ok(Address {
        interface: read_u32(payload, 4)?,
        address: decode_ip(family, &address.value)?,
        local,
        prefix_len: payload[1],
        flags: payload[2],
        scope: payload[3],
    })
}

fn decode_route(payload: &[u8]) -> Result<Route> {
    if payload.len() < 12 {
        return Err(Error::Decode("truncated rtmsg".into()));
    }
    let family = payload[0];
    let attributes = decode_attributes(&payload[12..])?;
    Ok(Route {
        family,
        destination: optional_ip(&attributes, RTA_DST, family)?,
        destination_prefix: payload[1],
        source: optional_ip(&attributes, RTA_SRC, family)?,
        source_prefix: payload[2],
        gateway: optional_ip(&attributes, RTA_GATEWAY, family)?,
        output_interface: optional_u32(&attributes, RTA_OIF)?,
        table: optional_u32(&attributes, RTA_TABLE)?.unwrap_or(payload[4].into()),
        priority: optional_u32(&attributes, RTA_PRIORITY)?,
        protocol: payload[5],
        scope: payload[6],
        route_type: payload[7],
    })
}

fn decode_rule(payload: &[u8]) -> Result<Rule> {
    if payload.len() < 12 {
        return Err(Error::Decode("truncated fib_rule_hdr".into()));
    }
    let family = payload[0];
    let attributes = decode_attributes(&payload[12..])?;
    Ok(Rule {
        family,
        destination: optional_ip(&attributes, FRA_DST, family)?,
        destination_prefix: payload[1],
        source: optional_ip(&attributes, FRA_SRC, family)?,
        source_prefix: payload[2],
        table: optional_u32(&attributes, FRA_TABLE)?.unwrap_or(payload[4].into()),
        priority: optional_u32(&attributes, FRA_PRIORITY)?,
        fwmark: optional_u32(&attributes, FRA_FWMARK)?,
        fwmask: optional_u32(&attributes, FRA_FWMASK)?,
        action: payload[7],
    })
}

fn optional_u32(attributes: &[Attribute], kind: u16) -> Result<Option<u32>> {
    attributes
        .iter()
        .find(|attribute| attribute.base_kind() == kind)
        .map(Attribute::as_u32)
        .transpose()
}

fn optional_ip(attributes: &[Attribute], kind: u16, family: u8) -> Result<Option<IpAddr>> {
    attributes
        .iter()
        .find(|attribute| attribute.base_kind() == kind)
        .map(|attribute| decode_ip(family, &attribute.value))
        .transpose()
}

fn decode_ip(family: u8, bytes: &[u8]) -> Result<IpAddr> {
    match i32::from(family) {
        libc::AF_INET if bytes.len() == 4 => Ok(IpAddr::V4(Ipv4Addr::new(
            bytes[0], bytes[1], bytes[2], bytes[3],
        ))),
        libc::AF_INET6 if bytes.len() == 16 => Ok(IpAddr::V6(Ipv6Addr::from(
            <[u8; 16]>::try_from(bytes)
                .map_err(|_| Error::Decode("invalid IPv6 address length".into()))?,
        ))),
        _ => Err(Error::Decode(format!(
            "unsupported address family {family} with {} bytes",
            bytes.len()
        ))),
    }
}

fn address_family(address: IpAddr) -> u8 {
    match address {
        IpAddr::V4(_) => libc::AF_INET as u8,
        IpAddr::V6(_) => libc::AF_INET6 as u8,
    }
}

fn ip_bytes(address: IpAddr) -> Vec<u8> {
    match address {
        IpAddr::V4(address) => address.octets().to_vec(),
        IpAddr::V6(address) => address.octets().to_vec(),
    }
}

#[cfg(all(test, feature = "tokio"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dumps_live_route_state() {
        let client = RouteClient::open().unwrap();
        assert!(
            client
                .links()
                .await
                .unwrap()
                .iter()
                .any(|link| link.name == "lo")
        );
        client.addresses().await.unwrap();
        client.routes().await.unwrap();
        client.rules().await.unwrap();
    }
}
