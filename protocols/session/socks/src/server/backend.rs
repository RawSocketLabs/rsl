/// Server configuration before an execution backend has been selected.
pub struct Unselected;

/// Blocking server builder state.
#[cfg(feature = "blocking")]
pub struct Blocking;

/// Tokio server builder state.
#[cfg(feature = "tokio")]
pub struct Tokio;
