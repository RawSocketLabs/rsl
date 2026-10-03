// --- Workspace dependencies ---
use bnb::{NormalizeEnumAliases, bin};
use thiserror::Error;

// --- Internal modules ---
use super::{AuthMethod, VERSION};

const MAX_METHODS: usize = u8::MAX as usize;

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
enum ValidationError {
    #[error("at least one authentication method is required")]
    EmptyMethods,
    #[error("authentication method count {count} exceeds {MAX_METHODS}")]
    TooManyMethods { count: usize },
    #[error("no-acceptable-methods cannot be offered by a client")]
    NoAcceptableMethod,
}

/// A client method-negotiation request: `VER`, `NMETHODS`, `METHODS` (RFC 1928 §3).
///
/// The builder requires at least one selectable method. Decoding remains permissive, while the
/// encoded method count is always derived and overflow-checked.
//~ models rfc1928#3 part="client method negotiation request"
#[bin(big, validate = MethodRequest::check_methods)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodRequest {
    /// The protocol version. The builder defaults to [`VERSION`].
    #[reserved_with(VERSION)]
    pub version: u8,
    /// Authentication methods offered by the client, in wire order.
    #[brw(count_prefix = u8)]
    pub methods: Vec<AuthMethod>,
}

impl MethodRequest {
    /// Check method count and client-selectable offers for builders and explicit validation.
    fn check_methods(&self) -> Result<(), ValidationError> {
        let methods = &self.methods;

        match methods.len() {
            0 => Err(ValidationError::EmptyMethods),
            count if count > MAX_METHODS => Err(ValidationError::TooManyMethods { count }),
            _ => Ok(()),
        }?;

        methods
            .iter()
            .copied()
            .all(|method| method.into_normalized_enum_aliases() != AuthMethod::NoAcceptable)
            .then_some(())
            .ok_or(ValidationError::NoAcceptableMethod)
    }
}

/// A server method selection: `VER`, `METHOD` (RFC 1928 §3).
//~ models rfc1928#3 part="server method selection"
#[bin(big)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MethodSelection {
    /// The protocol version. The builder defaults to [`VERSION`].
    #[reserved_with(VERSION)]
    pub version: u8,
    /// The selected method, or [`AuthMethod::NoAcceptable`].
    pub method: AuthMethod,
}

impl MethodSelection {
    /// Check the response version and the client's single explicitly offered method.
    pub(crate) fn check_offered(self, offered: AuthMethod) -> Result<(), crate::error::Error> {
        // --- Internal modules ---
        use crate::error::Error;

        super::validation::version(self.version, VERSION)?;
        match self.method {
            AuthMethod::NoAcceptable => Err(Error::NoAcceptableMethod),
            method if method == offered => Ok(()),
            method => Err(Error::UnexpectedMethod(method)),
        }
    }
}
