//! Local browser proxy: `cargo run -p socks --features blocking --example browser_proxy`.
//! An optional port argument replaces 1080; the listener always stays on IPv4 loopback.
//! Configure the browser for SOCKS5 at the printed address, with proxy-side DNS.
//! No authentication: every local application can reach any TCP target through this proxy.
//! Do not expose this policy on a LAN/public listener. Ctrl-C terminates the process;
//! this minimal example does not install a graceful signal handler.
use socks::{
    Server, Version,
    server::policy::{Policy, ServerAuth},
};
use std::{
    env,
    error::Error,
    io::{self, Write},
    net::{Ipv4Addr, TcpListener},
    sync::atomic::AtomicBool,
};

/// Run a loopback-only SOCKS5 listener with the library's bounded session handling.
fn main() -> Result<(), Box<dyn Error>> {
    let port = env::args()
        .nth(1)
        .map(|port| port.parse::<u16>())
        .transpose()?
        .unwrap_or(1080);

    // Broad destination access is deliberate for browsing, not a production allowlist.
    let policy = Policy::new(ServerAuth::no_authentication(), |_| true);
    let server = Server::configure()
        .protocols([Version::V5])
        .policy(policy)
        .blocking()
        .build()?;

    // Bind locally even when selecting a different port; never accept an arbitrary host.
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
    writeln!(
        io::stderr(),
        "SOCKS5 browser proxy: {}",
        listener.local_addr()?
    )?;
    writeln!(io::stderr(), "No authentication; stop with Ctrl-C.")?;

    // The listener manages concurrency and bidirectional relay; session failures stay isolated.
    let shutdown = AtomicBool::new(false);
    server.serve(listener, &shutdown, |error| {
        let _ = writeln!(io::stderr(), "SOCKS session failed: {error}");
    })?;
    Ok(())
}
