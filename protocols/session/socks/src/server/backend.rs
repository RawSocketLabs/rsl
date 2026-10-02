/// Server configuration before an execution backend has been selected.
pub struct Unselected;

/// Blocking server builder state.
#[cfg(feature = "blocking")]
pub struct Blocking;

/// Mio server builder state.
#[cfg(feature = "mio")]
pub struct Mio;

/// Tokio server builder state.
#[cfg(feature = "tokio")]
pub struct Tokio;
