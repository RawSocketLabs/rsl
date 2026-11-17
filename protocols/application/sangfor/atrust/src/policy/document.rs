//! The parsed policy and its lookups.

// --- Standard library ---
use std::cmp::Reverse;
use std::net::Ipv4Addr;
use std::sync::Arc;

// --- Internal modules ---
use super::{DomainRule, IpRule, NameRoute, NodeGroup, Transport};

/// The access policy of one session (spec §6): the rules, in document order, and the nodes
/// that serve them. Built by [`Policy::parse`](Self::parse).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Policy {
    /// The address rules, in document order. Order decides first and last matches.
    pub ip_rules: Vec<IpRule>,
    /// The name rules, in document order.
    pub domain_rules: Vec<DomainRule>,
    /// Addresses the policy pins to host names, in document order: normalised name, address.
    /// The pins of one address-list entry share their name.
    pub pins: Vec<(Arc<str>, Ipv4Addr)>,
    /// The DNS servers the policy names, primary first. Text as received.
    pub dns_servers: Vec<String>,
    /// The node groups.
    pub node_groups: Vec<NodeGroup>,
    /// The id of the group that serves a rule whose own group is unknown. May be empty.
    pub major_group: String,
}

impl Policy {
    /// The address rules that cover `address` over `transport` on `port`, in document order.
    /// The first match is `next()`, the last is `next_back()` (spec §6.5).
    pub fn ip_matches(
        &self,
        address: Ipv4Addr,
        transport: Transport,
        port: u16,
    ) -> impl DoubleEndedIterator<Item = &IpRule> {
        self.ip_rules
            .iter()
            .filter(move |rule| rule.covers(address, transport, port))
    }

    /// The name rules whose name selects `name`, most specific name first; rules that share
    /// a name stay in document order (spec §6.5). `name` is normalised here.
    #[must_use]
    pub fn domain_candidates(&self, name: &str) -> Vec<&DomainRule> {
        let name = DomainRule::normalise(name);
        let mut candidates: Vec<&DomainRule> = self
            .domain_rules
            .iter()
            .filter(|rule| rule.selects(&name))
            .collect();
        // Stable, so equal names keep document order.
        candidates.sort_by_key(|rule| Reverse(rule.name.len()));
        candidates
    }

    /// The name rule for `name` over `transport` on `port`: the first candidate that applies
    /// (spec §6.5). A more specific name whose rules do not apply does not hide a less
    /// specific one.
    #[must_use]
    pub fn domain_rule(&self, name: &str, transport: Transport, port: u16) -> Option<&DomainRule> {
        self.domain_candidates(name)
            .into_iter()
            .find(|rule| rule.admits(transport, port))
    }

    /// The addresses the policy pins to `name`, in document order. `name` is normalised here.
    pub fn pinned(&self, name: &str) -> impl Iterator<Item = Ipv4Addr> + '_ {
        let name = DomainRule::normalise(name);
        self.pins
            .iter()
            .filter(move |(host, _)| **host == *name)
            .map(|(_, address)| *address)
    }

    /// Whether a TCP connection to the host `name` on `port` goes through a TCP tunnel by a
    /// name rule (spec §6.6 step 5.1).
    ///
    /// A rule that leaves TCP to this tunnel is preferred over one that gives it to the L3
    /// tunnel. `take_all` is for a client with no L3 tunnel: it takes the second kind too.
    #[must_use]
    pub fn tcp_rule_for_name(&self, name: &str, port: u16, take_all: bool) -> NameRoute<'_> {
        let mut applicable = self
            .domain_candidates(name)
            .into_iter()
            .filter(|rule| rule.admits(Transport::Tcp, port))
            .peekable();
        let Some(first) = applicable.peek().copied() else {
            return NameRoute::Unmatched;
        };
        let rule = applicable
            .find(|rule| !rule.target.tcp_prefers_l3)
            .unwrap_or(first);
        if rule.target.tcp_prefers_l3 && !take_all {
            return NameRoute::PrefersL3;
        }
        NameRoute::Rule(rule)
    }

    /// The address rule that carries a TCP connection to `address` on `port` through a TCP
    /// tunnel: the last matching rule that leaves TCP to this tunnel, or, only with
    /// `take_all`, the last matching rule of any kind (spec §6.6 step 5.2).
    #[must_use]
    pub fn tcp_rule_for_address(
        &self,
        address: Ipv4Addr,
        port: u16,
        take_all: bool,
    ) -> Option<&IpRule> {
        let mut matches = self.ip_matches(address, Transport::Tcp, port);
        let last = matches.next_back()?;
        if !last.target.tcp_prefers_l3 {
            return Some(last);
        }
        matches
            .rfind(|rule| !rule.target.tcp_prefers_l3)
            .or(take_all.then_some(last))
    }

    /// The node to connect to for the group `group_id` when no node has been probed: that
    /// group's first node, else the major group's (spec §6.6 step 6). An empty id names no
    /// group, even one whose own id is empty.
    #[must_use]
    pub fn node(&self, group_id: &str) -> Option<&str> {
        let first_node = |id: &str| {
            self.node_groups
                .iter()
                .find(|group| !id.is_empty() && group.id == id)
                .and_then(NodeGroup::first_node)
        };
        first_node(group_id).or_else(|| first_node(&self.major_group))
    }
}
