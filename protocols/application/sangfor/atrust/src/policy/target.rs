//! What a rule sends its traffic to.

/// The application a rule belongs to, with the settings every rule of that application
/// shares (spec §6.2).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Target {
    /// The application's id, sent as `appId` when a tunnel is opened. Never empty.
    pub app_id: String,
    /// The node group that serves the application. Empty when the policy names none, in which
    /// case the major group serves it.
    pub node_group_id: String,
    /// Whether TCP to this application is meant for the L3 tunnel instead of a TCP tunnel.
    pub tcp_prefers_l3: bool,
    /// Whether the gateway resolves a destination name itself. When false the client resolves
    /// the name and sends the address.
    pub addr_pretend: bool,
}
