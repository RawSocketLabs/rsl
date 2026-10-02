/// Configuration state before a client backend is selected; cannot build a client.
pub struct Unselected;

/// Selected blocking backend for the generic client builder.
#[cfg(feature = "blocking")]
pub struct Blocking;

/// Selected Mio backend for the generic client builder.
#[cfg(feature = "mio")]
pub struct Mio;

/// Selected Tokio backend for the generic client builder.
#[cfg(feature = "tokio")]
pub struct Tokio;
