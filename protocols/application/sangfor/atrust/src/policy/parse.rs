//! Reading a policy document.

// --- Standard library ---
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;

// --- Workspace dependencies ---
use serde_json::Value;

// --- Internal modules ---
use super::json::{items, member, text};
use super::{DomainRule, Error, IpRule, NodeGroup, Policy, Ports, Protocol, Target, ports};

impl Policy {
    /// Read the policy document a gateway returns (spec §6.1 to §6.4).
    ///
    /// `server_host` is the gateway's host, without a port: it replaces the placeholder a
    /// node address may hold. Keys are matched without regard to letter case. A member that
    /// is missing or of the wrong type yields nothing, so a document with no usable content
    /// is an empty policy, not an error.
    ///
    /// Where the specification lets an implementation differ from the reference client, this
    /// one does:
    ///
    /// - an application with no id is dropped;
    /// - white space around a host is removed before the host is classified;
    /// - a pin's host is normalised like a queried name, and a host whose name begins with a
    ///   dot (a wildcard's suffix) gets no pins, since no queried name could find them;
    /// - a port number written with a sign is malformed;
    /// - a node with no address is dropped.
    ///
    /// # Errors
    ///
    /// [`Error::Json`] when `document` is not JSON.
    pub fn parse(document: &[u8], server_host: &str) -> Result<Self, Error> {
        let root: Value = serde_json::from_slice(document).map_err(Error::Json)?;
        let data = member(Some(&root), "data");
        let app_list = member(member(data, "appList"), "data");
        let mut policy = Self::default();

        for group in items(app_list, "appInfo") {
            for app in items(Some(group), "apps") {
                policy.add_app(app);
            }
        }

        let node_conf = member(member(app_list, "config"), "nodeGroupConf");

        let client_option = member(member(member(data, "sdpPolicy"), "data"), "clientOption");
        let first = member(client_option, "dnsOption");
        let servers = if text(first, "firstDNS").is_empty() {
            member(client_option, "dnsOptionV2")
        } else {
            first
        };
        let dns_servers = [text(servers, "firstDNS"), text(servers, "secondDNS")]
            .into_iter()
            .filter(|server| !server.is_empty())
            .map(str::to_owned)
            .collect();

        Ok(Self {
            dns_servers,
            major_group: text(member(node_conf, "majorNodeGroup"), "id").to_owned(),
            node_groups: items(node_conf, "nodeGroupList")
                .iter()
                .map(|group| NodeGroup::parse(group, server_host))
                .collect(),
            ..policy
        })
    }

    /// Add the rules of one application. Only `L3VPN` applications carry rules (spec §6.2).
    fn add_app(&mut self, app: &Value) {
        let app = Some(app);
        let app_id = text(app, "id");
        if text(app, "accessModel") != "L3VPN" || app_id.is_empty() {
            return;
        }
        let flag = |key| member(app, key).and_then(Value::as_bool);
        // Shared, not copied into each rule: a document is free to pair one long id with
        // many addresses.
        let target = Arc::new(Target {
            app_id: app_id.to_owned(),
            node_group_id: text(app, "nodeGroupId").to_owned(),
            tcp_prefers_l3: flag("enableTCPPrefL3").unwrap_or(false),
            // False only for the JSON boolean `false`.
            addr_pretend: flag("addrPretend") != Some(false),
        });
        for address in items(app, "addressList") {
            self.add_address(Some(address), &target);
        }
    }

    /// Add the rules and pins of one `addressList` entry. An entry with an unknown protocol
    /// or a malformed port yields nothing at all.
    fn add_address(&mut self, address: Option<&Value>, target: &Arc<Target>) {
        let Some(protocol) = Protocol::parse(text(address, "protocol")) else {
            return;
        };
        let Some(ports) = Ports::parse(text(address, "port")) else {
            return;
        };
        let mut add_range = |min, max| {
            self.ip_rules.push(IpRule {
                min,
                max,
                ports,
                protocol,
                target: Arc::clone(target),
            });
        };
        let name = match Host::classify(text(address, "host")) {
            Host::Nothing => return,
            Host::Range(min, max) => return add_range(min, max),
            Host::Name(name) => name,
        };
        // A pinned address is reachable as an address too, under the same application.
        let pinned: Vec<Ipv4Addr> = items(address, "ip")
            .iter()
            .filter_map(|ip| ip.as_str()?.parse().ok())
            .collect();
        for &ip in &pinned {
            add_range(ip, ip);
        }
        // A rule with no name is ignored, though its pinned addresses were still rules.
        if name.is_empty() {
            return;
        }
        // No queried host name begins with a dot, so a suffix's pins could never be found.
        if !name.starts_with('.') {
            let host: Arc<str> = Arc::from(name.as_str());
            self.pins
                .extend(pinned.into_iter().map(|ip| (Arc::clone(&host), ip)));
        }
        self.domain_rules.push(DomainRule {
            name,
            ports,
            protocol,
            target: Arc::clone(target),
        });
    }
}

/// What an address's `host` member turns out to be (spec §6.2).
enum Host {
    /// A range of IPv4 addresses: a literal, a CIDR block or `first-last`.
    Range(Ipv4Addr, Ipv4Addr),
    /// A host name, normalised; a leading dot marks a `*.` wildcard's suffix. It may be
    /// empty: such a host makes no name rule, but its pinned addresses still count.
    Name(String),
    /// Nothing usable: an IPv6 host, or a name with a `*` in an unsupported place.
    Nothing,
}

impl Host {
    /// Classify a `host` value: an address, then a CIDR block, then a range, else a name.
    fn classify(host: &str) -> Self {
        let host = host.trim();
        if let Some(address) = ipv4_literal(host) {
            return Self::Range(address, address);
        }
        if host.parse::<Ipv6Addr>().is_ok() {
            return Self::Nothing;
        }
        if let Some(block) = host
            .split_once('/')
            .and_then(|(address, prefix)| cidr(address, prefix))
        {
            return block;
        }
        if let Some(range) = host
            .split_once('-')
            .and_then(|(first, last)| range(first, last))
        {
            return range;
        }

        let name = DomainRule::normalise(host);
        let (wildcard, rest) = match name.strip_prefix("*.") {
            Some(rest) => (true, rest),
            None => (false, name.as_str()),
        };
        if rest.contains('*') {
            return Self::Nothing;
        }
        Self::Name(if wildcard { format!(".{rest}") } else { name })
    }
}

/// An IPv4 address in strict dotted decimal, or written as an IPv4-mapped IPv6 literal.
fn ipv4_literal(text: &str) -> Option<Ipv4Addr> {
    text.parse()
        .ok()
        .or_else(|| text.parse::<Ipv6Addr>().ok()?.to_ipv4_mapped())
}

/// `address/prefix` as a host: the block from its network address to its broadcast address
/// for IPv4, nothing for IPv6, and `None` when it is not a CIDR block at all.
fn cidr(address: &str, prefix: &str) -> Option<Host> {
    let prefix = ports::decimal(prefix)?;
    match address.parse().ok()? {
        IpAddr::V4(address) => {
            let prefix = u32::try_from(prefix).ok().filter(|prefix| *prefix <= 32)?;
            let host_bits = u32::MAX.checked_shr(prefix).unwrap_or(0);
            let network = u32::from(address) & !host_bits;
            Some(Host::Range(network.into(), (network | host_bits).into()))
        }
        IpAddr::V6(_) => (prefix <= 128).then_some(Host::Nothing),
    }
}

/// `first-last` as a host: an IPv4 range when both are IPv4 addresses, nothing when IPv6 is
/// involved, and `None` when either side is not an address (the host is then a name).
fn range(first: &str, last: &str) -> Option<Host> {
    match (first.parse().ok()?, last.parse().ok()?) {
        (IpAddr::V4(first), IpAddr::V4(last)) => Some(Host::Range(first, last)),
        _ => Some(Host::Nothing),
    }
}
