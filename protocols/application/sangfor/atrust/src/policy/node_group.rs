//! The nodes that carry tunnels.

// --- Workspace dependencies ---
use serde_json::Value;

// --- Internal modules ---
use super::json::{items, text};

/// The port a node address gets when it names none.
const DEFAULT_PORT: &str = ":441";
/// The node address that stands for the gateway itself.
const SERVER_PLACEHOLDER: &str = "{{sdpcHost}}";

/// A group of tunnel nodes (spec §6.4). Each address is `host:port`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct NodeGroup {
    /// The group's id, which rules refer to.
    pub id: String,
    /// The nodes reachable from outside, in document order.
    pub wan: Vec<String>,
    /// The nodes reachable from the gateway's own network, in document order.
    pub lan: Vec<String>,
}

impl NodeGroup {
    /// Read one `nodeGroupList` entry (spec §6.4): the placeholder that is a whole address
    /// becomes `server_host`, and an address with no port gets the default one. An entry
    /// with no address, or of a type other than `wan` and `lan`, is dropped.
    pub(super) fn parse(group: &Value, server_host: &str) -> Self {
        let group = Some(group);
        let mut nodes = Self {
            id: text(group, "id").to_owned(),
            wan: Vec::new(),
            lan: Vec::new(),
        };
        for info in items(group, "addressInfo") {
            let info = Some(info);
            let address = match text(info, "address") {
                "" => continue,
                SERVER_PLACEHOLDER => server_host,
                address => address,
            };
            let address = if address.contains(':') {
                address.to_owned()
            } else {
                format!("{address}{DEFAULT_PORT}")
            };
            match text(info, "type").to_ascii_lowercase().as_str() {
                "wan" => nodes.wan.push(address),
                "lan" => nodes.lan.push(address),
                _ => {}
            }
        }
        nodes
    }

    /// The node to use when none has been probed: the first outside node, else the first
    /// inside one.
    #[must_use]
    pub fn first_node(&self) -> Option<&str> {
        self.wan.first().or(self.lan.first()).map(String::as_str)
    }
}
