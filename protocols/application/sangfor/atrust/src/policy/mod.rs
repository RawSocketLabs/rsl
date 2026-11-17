//! The access policy (spec §6): which destinations the gateway serves, through which
//! application and node group, and the lookups a client makes against it.
//!
//! [`Policy::parse`] reads the document a gateway returns. Lookups are plain scans in
//! document order; nothing here does I/O or resolves names.

mod document;
mod domain_rule;
mod error;
mod ip_rule;
mod json;
mod name_route;
mod node_group;
mod parse;
mod ports;
mod protocol;
mod target;

pub use document::Policy;
pub use domain_rule::DomainRule;
pub use error::Error;
pub use ip_rule::IpRule;
pub use name_route::NameRoute;
pub use node_group::NodeGroup;
pub use ports::Ports;
pub use protocol::{Protocol, Transport};
pub use target::Target;
