//! What a user logs in with.

// --- Standard library ---
use std::fmt;

/// A user name and password for one login domain (spec §5.4).
#[derive(Clone, Copy)]
pub struct Credentials<'a> {
    /// The user name, without a domain.
    pub username: &'a str,
    /// The password.
    pub password: &'a str,
    /// The login domain, exactly as one of the gateway's password methods names it. There is
    /// no default: the gateway's list says which domains exist.
    pub domain: &'a str,
}

impl Credentials<'_> {
    /// The name the gateway knows the user by: the user name, `@`, the domain. Nothing is
    /// checked or escaped, so a user name that holds an `@` keeps it.
    #[must_use]
    pub fn qualified_username(&self) -> String {
        format!("{}@{}", self.username, self.domain)
    }
}

impl fmt::Debug for Credentials<'_> {
    /// Shows who is logging in, and never the password.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Credentials")
            .field("username", &self.username)
            .field("domain", &self.domain)
            .finish_non_exhaustive()
    }
}
