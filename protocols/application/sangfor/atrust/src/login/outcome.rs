//! What a completed login yields.

// --- Standard library ---
use std::fmt;

// --- Internal modules ---
use super::AntiMitm;
use crate::policy::Policy;

/// The result of a login (spec §5.2). The session itself stays in the
/// [`Session`](super::Session), which later control-plane requests need.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
#[must_use = "the anti-MITM outcome says whether the gateway passed its checks"]
pub struct Login {
    /// The session identifier: the one value of the session a tunnel sends.
    pub sid: String,
    /// The user's name as the gateway knows it. It may be empty.
    pub user_name: String,
    /// The access policy.
    pub policy: Policy,
    /// Whether TCP tunnels use the framed, zero-RTT data phase.
    pub zero_rtt: bool,
    /// What the anti-MITM checks on the login configuration found. Unless the login was
    /// told to stop on a failure, a failure is reported here and nowhere else: the caller
    /// decides whether to use a session with a gateway that failed them.
    pub anti_mitm: AntiMitm,
}

impl fmt::Debug for Login {
    /// Shows everything but the session identifier, which is all a tunnel needs to act as
    /// the user.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Login")
            .field("user_name", &self.user_name)
            .field("policy", &self.policy)
            .field("zero_rtt", &self.zero_rtt)
            .field("anti_mitm", &self.anti_mitm)
            .finish_non_exhaustive()
    }
}
