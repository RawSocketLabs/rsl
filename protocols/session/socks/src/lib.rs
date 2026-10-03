//! SOCKS protocol wire codecs on [`bnb`].
//!
//! The current surface covers RFC 1928 SOCKS5 method negotiation, requests, replies, registries,
//! and typed IPv4/domain/IPv6 endpoints plus RFC 1929 username/password messages.
//! The optional `blocking`, `tokio`, and `mio` features add CONNECT clients, embeddable
//! server sessions, and bounded listening proxies. The default provides codecs and client configuration without transport support.
//!
//! # Guide: how SOCKS works
//!
//! SOCKS lets an application ask a proxy to reach a destination on its behalf.
//! For CONNECT, the application connects to the proxy first; the proxy then opens
//! a separate connection to the requested destination. Application data flows through
//! those connections after negotiation succeeds. The proxy address and destination
//! address are therefore different inputs, even when both are expressed as IP/port pairs.
//!
//! ## Negotiation before application data
//!
//! A SOCKS5 CONNECT session proceeds through these steps:
//!
//! 1. **Choose authentication.** The client offers methods and the server selects one.
//!    If none is acceptable, the connection ends.
//! 2. **Authenticate when required.** Username/password uses a separate exchange;
//!    selecting no authentication skips that exchange, not destination authorization.
//! 3. **Request an operation.** The client sends a command and destination. The server
//!    evaluates its policy before attempting the requested operation.
//! 4. **Check the reply.** A successful CONNECT reply permits application traffic;
//!    a failure is not an established tunnel. Its bound address describes the proxy's
//!    outbound socket, not necessarily its listening address or the destination.
//!
//! See [RFC 1928 §§3–6](https://www.rfc-editor.org/rfc/rfc1928.html#section-3) and
//! [RFC 1929 §2](https://www.rfc-editor.org/rfc/rfc1929.html#section-2).
//!
//! ## Commands and this crate's scope
//!
//! | Command | Purpose | Implemented surface |
//! | --- | --- | --- |
//! | CONNECT | Ask the proxy to establish an outbound TCP connection | Configured clients/servers and complete proxies on all three backends |
//! | BIND | Ask the proxy to listen for an incoming connection; two replies report listening and the peer connection | Version-specific blocking embedded APIs and bounded TCP helpers only |
//! | UDP ASSOCIATE | Establish a UDP relay association tied to the control TCP connection | Command code is representable; no operational implementation |
//!
//! These commands are specified in [RFC 1928 §§4 and 6](https://www.rfc-editor.org/rfc/rfc1928.html#section-4).
//! SOCKS4/4A and GSS-API authentication are not implemented. This is a supported subset,
//! not a claim to implement every requirement of SOCKS5.
//!
//! ## Addresses, authentication, and policy
//!
//! SOCKS5 addresses can carry IPv4, IPv6, or a domain name together with a port.
//! [`Destination`] is the application-facing address; [`v5::Endpoint`] also models
//! the wire address-type discriminator. Forwarding a domain to the proxy avoids
//! requiring the client to resolve it first. Wire-domain bytes are preserved;
//! this is not DNS-label or IDNA validation.
//! See [RFC 1928 §5](https://www.rfc-editor.org/rfc/rfc1928.html#section-5).
//!
//! Authentication and authorization answer different questions: accepting credentials
//! does not grant access to every destination. Configured servers use one explicit
//! policy across accepted protocol versions and authorize before dialing. No-authentication
//! must also be selected explicitly. A successful credential check is not a principal/identity API.
//!
//! Username/password authentication sends credentials in cleartext; this crate does
//! not add encryption. Use an appropriately protected transport/network when credentials
//! need confidentiality, and application TLS when end-to-end protection is required.
//! See [RFC 1929 §3](https://www.rfc-editor.org/rfc/rfc1929.html#section-3).
//!
//! ## Choose your API level
//!
//! - **Configured clients and servers:** start at `Client::configure` or `Server::configure`,
//!   select a backend, then build. Construction performs no network I/O. A client connection
//!   supports ordinary backend-appropriate I/O; a server connection owns both relay sides.
//! - **Version-specific exchanges:** use `v5::client` and `v5::server` when embedding the
//!   handshake in your own transport or policy workflow. These APIs do not replace the
//!   complete proxy's authorization, listener, and relay infrastructure.
//! - **Wire messages:** use [`v5::wire`] for construction, encoding, and decoding without
//!   transport I/O. Builders guide compliant construction; raw values and decoding preserve
//!   representable noncanonical data. Encoding alone is not a connection or authorization check.
//!
//! Keep the returned buffered stream or connection when handing off application I/O:
//! negotiation can read ahead into application data. Extracting only the raw socket
//! can discard that retained prefix.
//!
//! # Guide: using the library
//!
//! Start at the highest level that owns the work you want delegated. These are alternative
//! entry points, not steps every application must implement.
//!
//! | Level | Library owns | Your application owns |
//! | --- | --- | --- |
//! | Configured client | Proxy negotiation and lossless connection handoff | Proxy/destination selection and application I/O |
//! | Complete proxy | Acceptance, configured policy enforcement, dialing, and relay | Listener address, policy, limits, shutdown, and error reporting |
//! | Configured server exchange | Negotiation, authorization, dialing, and success reply | Accepted socket and when to relay or take both connections |
//! | Version-specific exchange | One version's handshake and wire replies | Transport lifetime, deadlines, and server authorization/dialing |
//! | Wire codecs | Message construction, encoding, and decoding | Framing, transport, sequencing, and all policy |
//!
//! ## Connect an application through a proxy
//!
//! Enable `blocking` for this example. Configuration is reusable; each `connect` call
//! establishes a separate connection. The proxy is a numeric socket address, while a domain
//! destination is forwarded to the proxy for resolution.
//!
//! ```no_run
//! # #[cfg(feature = "blocking")]
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use socks::{Client, Destination, v5::client::Config};
//! use std::{io::{Read, Write}, time::Duration};
//!
//! // Choose authentication and a backend without opening a socket.
//! let client = Client::configure(Config::no_authentication())
//!     .blocking()
//!     .build()?;
//!
//! let proxy = "127.0.0.1:1080".parse()?;
//! let dest = Destination::domain(b"example.com", 80);
//! let mut connection = client.connect(proxy, dest)?;
//!
//! // Application deadlines are separate from the completed handshake.
//! connection.get_ref().set_read_timeout(Some(Duration::from_secs(10)))?;
//! connection.get_ref().set_write_timeout(Some(Duration::from_secs(10)))?;
//! connection.write_all(b"GET / HTTP/1.0\r\nHost: example.com\r\n\r\n")?;
//! let mut response = [0; 4096];
//! let received = connection.read(&mut response)?;
//! # let _ = received;
//! # Ok(()) }
//! ```
//!
//! To require credentials, use `Config::username_password(username, password)?` instead.
//! It does not silently fall back to no authentication. For an already connected transport,
//! use `client.connect_with(stream, dest)`; its deadlines are your responsibility.
//! The resulting `client::Connection` owns one application-facing buffered stream and the
//! proxy's bound address. It is not a listening client or a server-side relay pair.
//!
//! ## Select the execution model
//!
//! Tokio uses the same configuration and destination types, but negotiation and subsequent
//! I/O are asynchronous. Enable `tokio` and call this inside your runtime:
//!
//! ```no_run
//! # #[cfg(feature = "tokio")]
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use socks::{Client, Destination, v5::client::Config};
//! use tokio::io::AsyncWriteExt;
//!
//! let client = Client::configure(Config::no_authentication()).tokio().build()?;
//! let proxy = "127.0.0.1:1080".parse()?;
//! let dest = Destination::domain(b"example.com", 80);
//!
//! // The negotiated connection implements Tokio's application-I/O traits.
//! let mut connection = client.connect(proxy, dest).await?;
//! connection.write_all(b"GET / HTTP/1.0\r\nHost: example.com\r\n\r\n").await?;
//! # Ok(()) }
//! ```
//!
//! Apply application timeouts/cancellation around your Tokio I/O; completing negotiation
//! does not impose a lifetime limit on application traffic.
//!
//! Mio is a different driving contract, not an async function with a different name:
//!
//! 1. Build with `.mio().build()?`, establish a nonblocking transport, and call `connect_with`.
//! 2. Drive the returned handshake with `advance()`, registering its `interest()` in your poll.
//!    When `needs_advance()` is true, reschedule without waiting for another readiness edge.
//! 3. After completion, call `take_connection()` once. Keep the original poll alive and use
//!    it for the connection; handle `WouldBlock` and retain partial-write offsets yourself.
//!
//! The repository's `examples/mio_client.rs` demonstrates this entire lifecycle, including
//! TCP establishment through `io::mio::Connector`. Run it with
//! `cargo run -p socks --features mio --example mio_client -- 127.0.0.1:1080 example.com 80`.
//! If you deliberately want blocking setup, `connect_blocking(proxy, dest)` instead returns
//! the owned poll together with a negotiated connection; it is not the native event-loop path.
//!
//! ## Run a complete proxy
//!
//! Use `proxy` when you want acceptance and relay managed together. This blocking example
//! deliberately listens only on loopback and permits only loopback port 8080 targets.
//! Replace that demonstration policy with your application's explicit allowlist.
//!
//! ```no_run
//! # #[cfg(feature = "blocking")]
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use socks::{Server, Version, server::policy::{Policy, ServerAuth}};
//! use std::{net::TcpListener, sync::atomic::AtomicBool};
//!
//! // Authorize the resolved numeric target, not just the requested domain name.
//! let policy = Policy::new(ServerAuth::no_authentication(), |context| {
//!     context.target.ip().is_loopback() && context.target.port() == 8080
//! });
//! let server = Server::configure()
//!     .protocols([Version::V5])
//!     .policy(policy)
//!     .blocking()
//!     .build()?;
//!
//! // A supervisor sets this flag to stop acceptance and close active sessions.
//! let shutdown = AtomicBool::new(false);
//! let listener = TcpListener::bind("127.0.0.1:1080")?;
//! server.serve(listener, &shutdown, |error| {
//!     eprintln!("SOCKS session failed: {error}");
//! })?;
//! # Ok(()) }
//! ```
//!
//! Set resource/phase bounds with the server builder's `limits` option.
//! Blocking builders additionally offer `.blocking().thread_budget(32)`, reserving two
//! session/relay threads per connection before acceptance. The default budget is 128;
//! actual admission uses the smaller of `limits.connections` and half the thread budget.
//! This is per listener, excludes the calling thread, and does not bound direct
//! `serve_connection` calls or threads started by application callbacks. Tokio/Mio do not
//! expose this setting. For credentials,
//! supply `ServerAuth::username_password(verifier)` to `Policy`; secure password storage
//! and verification are yours. Callbacks must be fast, nonblocking, and non-panicking.
//! A Tokio server offers an owned lifecycle; constructing the proxy does not start it:
//!
//! ```no_run
//! # #[cfg(feature = "tokio")]
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use socks::{Server, Version};
//! use socks::server::policy::{Policy, ServerAuth};
//! use tokio::net::TcpListener;
//!
//! // Keep this example local and explicitly restrict outbound destinations.
//! let policy = Policy::new(ServerAuth::no_authentication(), |context| {
//!     context.target.ip().is_loopback()
//! });
//! let server = Server::configure()
//!     .protocols([Version::V5])
//!     .policy(policy)
//!     .tokio()
//!     .build()?;
//!
//! // Start acceptance independently of diagnostic consumption.
//! let listener = TcpListener::bind("127.0.0.1:1080").await?;
//! let mut running = server.into_proxy(listener).start()?;
//! let shutdown = running.shutdown_handle();
//!
//! // Give a clone to application shutdown control; request is idempotent.
//! shutdown.request();
//! while let Some(error) = running.next_session_error().await {
//!     eprintln!("session failed: {error}");
//! }
//! let dropped = running.take_dropped_count();
//! if dropped != 0 {
//!     eprintln!("{dropped} diagnostics dropped");
//! }
//!
//! // Diagnostic EOF is not a success result; join confirms cleanup and returns service status.
//! running.join().await?;
//! # Ok(())
//! # }
//! ```
//!
//! The queue retains at most 64 session errors and never blocks the service. Overflow
//! is retained in a separately sampled, saturating counter. You can also request
//! shutdown and join without consuming diagnostics. Dropping `running` requests shutdown,
//! but only awaiting `join` confirms cleanup; the runtime must remain alive.
//! Shutdown aborts active sessions rather than waiting for traffic to finish. Existing
//! shutdown priority is preserved: failures ready alongside shutdown or arising during
//! cleanup are discarded, rather than overriding successful shutdown.
//! For caller-managed acceptance, use `proxy::tokio::serve_connection` per socket.
//! The former callback-based Tokio `serve` APIs are replaced by this owned lifecycle.
//! A Mio server's `server.into_proxy(listener)?` returns an owned `proxy::mio::Proxy`:
//! drive it with `run` or `poll`, retaining access to its shutdown handle. See
//! `examples/mio_proxy.rs` for setup and shutdown. The server selects the backend once;
//! these methods delegate to the matching proxy infrastructure. Existing `proxy` free
//! functions and `Proxy::new` remain available for explicit driver composition.
//!
//! ### Choose a diagnostic reporting cadence
//!
//! Session errors are terminal for that session, not instructions to stop the proxy
//! or retry traffic. Classify/count routine failures and sample noisy logs; use `join`
//! for service status. Diagnostics currently contain no peer/destination metadata.
//! Keep reporting fast: slow logging can fill the 64-error queue. Lost details cannot
//! be recovered from the counter, which saturates at `usize::MAX`.
//!
//! | Need | Tool |
//! |---|---|
//! | Inspect each retained failure | `next_session_error()` |
//! | Amortize metrics updates under load | Sample `take_dropped_count()` after a batch |
//! | Report losses periodically, even when errors stop arriving | Sample on a Tokio interval |
//! | Ignore individual diagnostics but ensure shutdown completes | Request shutdown, then `join` |
//! | React to service failure | Inspect the result of `join`, independently of diagnostics |
//!
//! **Batch recipe:** here 32 means delivered session errors, not queue occupancy or
//! a threshold of dropped errors. It is an application policy, not a library default.
//! A partial batch can wait indefinitely while the service is quiet; always sample
//! again at EOF. Sampling resets the counter, so export it as a metric increment.
//!
//! ```no_run
//! # #[cfg(feature = "tokio")]
//! # async fn batch(mut running: socks::proxy::tokio::Running) -> Result<(), socks::error::Error> {
//! let mut processed = 0;
//! while let Some(error) = running.next_session_error().await {
//!     eprintln!("session failed: {error}");
//!     processed += 1;
//!
//!     // Amortize loss reporting across a fixed batch of delivered errors.
//!     if processed == 32 {
//!         let dropped = running.take_dropped_count();
//!         if dropped != 0 {
//!             eprintln!("{dropped} diagnostics dropped");
//!         }
//!         processed = 0;
//!     }
//! }
//!
//! // Include losses from the last partial batch, even on fatal termination.
//! let dropped = running.take_dropped_count();
//! if dropped != 0 {
//!     eprintln!("{dropped} diagnostics dropped");
//! }
//! running.join().await?;
//! # Ok(())
//! # }
//! ```
//!
//! **Timer recipe:** use when reporting should not depend on subsequent failures.
//! This needs Tokio's time driver. The interval starts after one period and skips
//! missed ticks rather than producing a burst after a stall. Leave `select!` unbiased
//! so a continuously ready error queue does not systematically starve the timer.
//!
//! ```no_run
//! # #[cfg(feature = "tokio")]
//! # async fn periodic(mut running: socks::proxy::tokio::Running) -> Result<(), socks::error::Error> {
//! use std::time::Duration;
//! use tokio::time::{Instant, MissedTickBehavior, interval_at};
//!
//! let period = Duration::from_secs(5);
//! let mut tick = interval_at(Instant::now() + period, period);
//! tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
//!
//! loop {
//!     tokio::select! {
//!         error = running.next_session_error() => {
//!             match error {
//!                 Some(error) => eprintln!("session failed: {error}"),
//!                 None => break,
//!             }
//!         }
//!         _ = tick.tick() => {
//!             let dropped = running.take_dropped_count();
//!             if dropped != 0 {
//!                 eprintln!("{dropped} diagnostics dropped");
//!             }
//!         }
//!     }
//! }
//!
//! // EOF may arrive before the next tick; do not lose the final sample.
//! let dropped = running.take_dropped_count();
//! if dropped != 0 {
//!     eprintln!("{dropped} diagnostics dropped");
//! }
//! running.join().await?;
//! # Ok(())
//! # }
//! ```
//!
//! In both recipes, give a shutdown-handle clone to application lifecycle control
//! before entering the loop. Receipt is cancellation-safe when the timer branch wins.
//! Neither cadence guarantees timely reporting if the runtime or reporting code stalls.
//! These are compositions of the baseline API, not extra Cargo features or background
//! metrics tasks. Batch and timer sampling can also be combined: each sample consumes
//! only losses accumulated since the preceding sample.
//!
//! ## Use the proxy from Firefox or another browser
//!
//! The runnable `examples/browser_proxy.rs` uses the same configured server and complete
//! blocking listener, but deliberately allows every TCP destination for local browsing:
//!
//! ```text
//! cargo run -p socks --features blocking --example browser_proxy
//! ```
//!
//! It listens on `127.0.0.1:1080`. Run it on the same machine as the browser; an optional
//! port can follow `--`, for example `-- 1081`. The bind address remains loopback-only.
//!
//! In Firefox Settings, search for **proxy** and open Connection Settings:
//!
//! 1. Choose **Manual proxy configuration**.
//! 2. Set **SOCKS Host** to `127.0.0.1`, **Port** to `1080`, and select **SOCKS v5**.
//! 3. Leave the HTTP/HTTPS proxy fields empty; this is SOCKS, not an HTTP proxy.
//! 4. Enable **Proxy DNS when using SOCKS v5** to send destination names to the proxy.
//! 5. Review **No Proxy For**: matching destinations bypass your proxy configuration.
//!
//! See [Mozilla's connection settings guide](https://support.mozilla.org/en-US/kb/connection-settings-firefox)
//! and [Firefox's SOCKS/DNS setting labels](https://searchfox.org/firefox-main/source/browser/locales/en-US/browser/preferences/connection.ftl).
//! Other browsers need equivalent SOCKS5 settings, which may be supplied by their OS.
//! Restore your previous proxy settings when you stop the example with Ctrl-C.
//!
//! This example allows any local process to reach any TCP target, including internal services.
//! It has no authentication and must not be exposed by changing the listener to a LAN/public
//! address. It forwards HTTP and HTTPS TCP connections without terminating browser TLS;
//! SOCKS itself adds no encryption. It is not a VPN, does not change your public egress IP
//! when run locally, and does not guarantee that every browser subsystem uses the proxy.
//! UDP ASSOCIATE is not implemented, so it cannot carry UDP-based traffic such as QUIC.
//! Default limits allow 64 simultaneous sessions and a five-minute total relay lifetime
//! per connection, not an idle timeout; long-lived connections will be closed at that limit.
//! Ctrl-C stops the process, rather than performing a graceful drain.
//!
//! ## Manage one accepted connection
//!
//! Use a configured blocking/Tokio server when your application already owns acceptance.
//! Its stages ensure destination authorization precedes dialing. This is a lower lifecycle
//! level than the complete proxy APIs, but it still applies the configured policy:
//!
//! ```no_run
//! # #[cfg(feature = "blocking")]
//! # fn example(server: &socks::server::blocking::Server, socket: std::net::TcpStream)
//! #     -> Result<(), socks::error::Error> {
//! // Read and authenticate the request, then resolve and authorize its targets.
//! let exchange = server.exchange(socket)?;
//! let authorized = exchange.authorize()?;
//!
//! // Dial an authorized target and acknowledge success before application traffic.
//! let connection = authorized.connect()?;
//! let (client_stream, target_stream) = connection.into_parts();
//! // Pass both sides to your relay; keep client_stream's buffered prefix intact.
//! # let _ = (client_stream, target_stream);
//! # Ok(()) }
//! ```
//!
//! Instead of taking the pair apart, call the server connection's `relay` method with
//! your relay timeout. Tokio exposes the corresponding stages with `.await`. Mio's configured
//! server is settings for the complete proxy, not this staged exchange API.
//!
//! Blocking servers also expose `authorized.dial()?` when socket registration or inspection
//! must happen before success. The returned `server::blocking::Connected` owns the target;
//! `target()` borrows it, and `send_success()` consumes the stage to return a relay connection.
//! If preparation fails, `connected.fail(error)` closes the target and attempts a failure
//! reply, returning the cause unless reply I/O fails. Dropping the stage sends no success.
//! `authorized.connect()` remains the convenience form of `dial()?.send_success()?`.
//!
//! ## Embed only SOCKS5 negotiation
//!
//! Use `v5::client` or `v5::server` when you own transport setup and server-side decisions.
//! The blocking client example accepts any `Read + Write` transport already connected to
//! the proxy; it does not open another socket:
//!
//! ```no_run
//! # #[cfg(feature = "blocking")]
//! # fn example<S: std::io::Read + std::io::Write>(transport: S)
//! #     -> Result<(), Box<dyn std::error::Error>> {
//! use socks::v5::{Endpoint, auth::ClientAuth, client::blocking};
//!
//! let dest = Endpoint::domain(b"example.com".to_vec(), 80)?;
//! let (stream, bound) = blocking::connect_with(transport, dest, ClientAuth::NoAuthentication)?;
//!
//! // Transfer the negotiated stream, including bytes read ahead during the handshake.
//! let (transport, buffered) = stream.into_parts();
//! let stream = socks::Stream::from_parts(transport, buffered);
//! # let _ = (stream, bound);
//! # Ok(()) }
//! ```
//!
//! On the embedded server path, `v5::server::blocking::exchange(stream, &auth)` returns
//! an authenticated request. Inspect `destination()` and `authentication()`, authorize
//! the exact resolved target, and dial it yourself. Only then call `send_success(bound)`
//! with the outbound socket's local endpoint; denial or dial failure needs `send_failure(code)`.
//! Unlike configured server stages, this API does not enforce your authorization decision.
//! You also own deadlines and relay. Tokio provides asynchronous exchanges; embedded Mio
//! exposes a resumable state machine rather than those blocking methods.
//!
//! Blocking embedded BIND is a separate two-reply workflow under `v5::client::blocking`
//! and `v5::server::blocking`; it is not an option on configured CONNECT clients/proxies.
//!
//! ## Work directly with wire messages
//!
//! Use [`v5::wire`] without any transport feature when you need message inspection,
//! custom sequencing, or protocol experiments. The [wire-codec example](#wire-codec-example)
//! below constructs a request and round-trips its bytes; it performs no negotiation.
//! Your code supplies message boundaries to `decode_exact`, orders the exchanges, checks
//! peer replies, and enforces policy. Use [`bnb`] incremental decoding when supplying bytes
//! gradually instead of assuming one transport read equals one message.
//!
//! ## Preserve buffered data across abstraction boundaries
//!
//! `client::Connection::into_parts()` returns `(Stream<S>, Destination)`, whereas the
//! server connection returns `(Stream<S>, S)` for client and target. `Stream::into_parts()`
//! goes one level lower and returns `(S, bnb::BitBuf)`; retain both and reconstruct with
//! `Stream::from_parts` as above. Reading directly through `get_mut()` bypasses buffered
//! bytes: use it for socket configuration/registration, not application reads.
//! `Stream::try_into_inner()` only extracts the transport when no unread prefix remains.
//! Guided negotiation errors are terminal: propagate them and discard the failed session,
//! rather than retrying negotiation on a partially consumed stream.
//!
//! # Module groups
//!
//! ## Setup and policy
//!
//! `client` owns configured clients; `server` owns configured servers and shared policy.
//! Both group their execution choices under a **Backends** section in their module docs.
//!
//! ## Protocol versions and wire messages
//!
//! [`v5`] groups version-specific client/server exchanges, authentication, and [`v5::wire`].
//! Wire types are also re-exported directly from `v5`.
//!
//! ## Proxy infrastructure and transport
//!
//! `proxy` groups listener services and runtime coordination by backend; established
//! server connections own their relay methods. `io` owns shared transport mechanics:
//! deadlines, TCP setup, buffering, and lossless handoff—not a single runtime-neutral driver.
//! [`error`] contains shared guided-API failures.
//!
//! ## Backend features
//!
//! | Feature | Execution model |
//! | --- | --- |
//! | `blocking` | Synchronous standard-library I/O |
//! | `tokio` | Async Tokio I/O |
//! | `mio` | Readiness-driven exchanges and a complete owned-poll proxy |
//!
//! Enable the features you need; they can coexist. With none enabled, wire codecs and
//! client configuration remain available, but a configured client cannot be built.
//!
//! # Wire-codec example
//!
//! ```
//! use socks::v5::{Command, Endpoint, Request};
//! use std::net::Ipv4Addr;
//!
//! let request = Request::builder()
//!     .command(Command::Connect)
//!     .destination(Endpoint::addr(Ipv4Addr::LOCALHOST, 443))
//!     .build()
//!     .unwrap();
//!
//! // Encode the request, then decode the same wire representation.
//! let wire = request.to_bytes().unwrap();
//! assert_eq!(Request::decode_exact(&wire).unwrap(), request);
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod client;
pub mod error;
pub mod io;
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
pub mod proxy;
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
pub mod server;
mod types;
/// SOCKS version 5 wire types (RFC 1928 and RFC 1929).
pub mod v5;

// --- Internal modules ---
pub use client::Client;
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
pub use io::stream::Stream;
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
pub use server::{Connection, Server};
pub use types::{Destination, Version};
