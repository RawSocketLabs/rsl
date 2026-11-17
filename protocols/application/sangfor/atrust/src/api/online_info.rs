//! The answer to `onlineInfo` (E22).

// --- Workspace dependencies ---
use http1::Response;
use serde::Deserialize;

// --- Internal modules ---
use super::members::or_default;
use super::{Envelope, Error};

/// Who a session belongs to (spec §4.3 E22, §5.2).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct OnlineInfo {
    /// The user's name as the gateway knows it. It may be empty, and a login with an empty
    /// name still succeeds.
    #[serde(deserialize_with = "or_default")]
    pub username: String,
}

impl OnlineInfo {
    /// Read an `onlineInfo` answer. The HTTP status is not looked at; the code must be zero.
    ///
    /// # Errors
    ///
    /// [`Error::Json`] when the body is not the JSON this endpoint returns, and
    /// [`Error::SessionInvalid`] or [`Error::Refused`] for a code other than zero.
    pub fn parse(response: &Response) -> Result<Self, Error> {
        let envelope = Envelope::parse(&response.body)?;
        envelope.accepted()?;
        envelope.data()
    }
}
