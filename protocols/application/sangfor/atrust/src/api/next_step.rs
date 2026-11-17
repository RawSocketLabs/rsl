//! The next-step fields of an authentication answer.

// --- Workspace dependencies ---
use http1::Response;
use serde::Deserialize;

// --- Internal modules ---
use super::members::{objects, or_default};
use super::{Envelope, Error};

/// The fields with which an answer names the next authentication step (spec §4.3 E11, §5.5).
/// Resolving them into a service is the login sequence's job.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct NextStep {
    /// The next service, possibly empty.
    #[serde(deserialize_with = "or_default")]
    pub next_service: String,
    /// The candidates for the next step.
    #[serde(deserialize_with = "objects")]
    pub next_service_list: Vec<NextEntry>,
}

/// One entry of `nextServiceList`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct NextEntry {
    /// The id of the step.
    #[serde(deserialize_with = "or_default")]
    pub auth_id: String,
    /// The step's service.
    #[serde(deserialize_with = "or_default")]
    pub auth_type: String,
    /// A refinement of the service.
    #[serde(deserialize_with = "or_default")]
    pub sub_type: String,
}

impl NextStep {
    /// Read the answer of an authentication step. The HTTP status is not looked at; the code
    /// must be zero (spec §4.3 notes).
    ///
    /// # Errors
    ///
    /// [`Error::Json`] when the body is not the JSON these endpoints return, and
    /// [`Error::SessionInvalid`] or [`Error::Refused`] for a code other than zero.
    pub fn parse(response: &Response) -> Result<Self, Error> {
        let envelope = Envelope::parse(&response.body)?;
        envelope.accepted()?;
        envelope.data()
    }
}
