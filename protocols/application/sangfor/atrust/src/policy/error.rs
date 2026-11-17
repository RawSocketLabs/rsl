//! Why a policy document could not be read.

// --- Workspace dependencies ---
use thiserror::Error as ThisError;

/// A policy document that could not be read at all.
///
/// Members that are missing or of the wrong type are not errors: they yield no rules.
#[derive(Debug, ThisError)]
#[non_exhaustive]
pub enum Error {
    /// The document is not JSON.
    #[error("the policy document is not valid JSON")]
    Json(#[source] serde_json::Error),
}
