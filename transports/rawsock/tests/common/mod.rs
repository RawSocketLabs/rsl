//! Helpers shared by the privileged-backend tests.

use std::env;

/// Whether a privileged test should run, given its layer's `capabilities()` flag.
///
/// Unprivileged runs skip (`false`). With `RAWSOCK_PRIV_TESTS` set, a missing privilege fails
/// the test instead, so a misconfigured namespace run cannot pass by skipping everything.
pub(crate) fn privileged(available: bool) -> bool {
    assert!(
        available || env::var_os("RAWSOCK_PRIV_TESTS").is_none(),
        "RAWSOCK_PRIV_TESTS is set but the layer is unavailable (no CAP_NET_RAW?)"
    );
    available
}
