/*!
A from-scratch implementation of the **SOCKS protocol, version 5** — the proxy
protocol that lets a client ask an intermediary to open TCP connections (CONNECT,
BIND) or relay UDP datagrams (UDP ASSOCIATE) on its behalf, optionally
authenticating first.

The crate is built in three layers, each usable on its own:

| Layer | What it gives you | Start at |
|---|---|---|
| **High-level** | Drive or serve a proxy: CONNECT, BIND, UDP ASSOCIATE | [`client::Client`], [`server::Server`] |
| **Mid-level** | Pluggable authentication method negotiation | [`auth::Authenticator`], [`auth::AuthHandler`] |
| **Low-level** | Encode/decode every SOCKS5 wire message | [`v5`] |

# Quick start

Connect to a host *through* a SOCKS5 proxy and speak to it as if the stream were
direct — the returned [`TcpStream`](std::net::TcpStream) is transparent:

```no_run
use std::io::{Read, Write};
use socks::client::Client;

# fn main() -> Result<(), socks::error::SocksError> {
// A proxy listening on localhost:1080, no authentication.
let mut stream = Client::new("127.0.0.1:1080".parse()?)
    .connect(("example.com", 80))?;

stream.write_all(b"GET / HTTP/1.0\r\nHost: example.com\r\n\r\n")?;
let mut response = Vec::new();
stream.read_to_end(&mut response)?;
# Ok(())
# }
```

Serving is the mirror image — see [`server::Server`]. Username/password
authentication (RFC 1929) is one call away with
[`Client::with_user_pass`](client::Client::with_user_pass).

# Standards

This crate tracks the SOCKS5 family of RFCs. Requirement-level provenance is
maintained out-of-band in the workspace's `protoref` compliance ledger; the
prose here cites RFC sections so the behavior is traceable from the docs alone.

| RFC | Title | Status in this crate |
|---|---|---|
| [RFC 1928] | SOCKS Protocol Version 5 | Implemented (CONNECT, BIND, UDP ASSOCIATE) |
| [RFC 1929] | Username/Password Authentication for SOCKS V5 | Implemented ([`auth`]) |
| [RFC 1961] | GSS-API Authentication Method for SOCKS V5 | Not implemented — see below |

GSS-API is intentionally out of scope for this crate so it stays
dependency-light; the [`auth::Authenticator`] / [`auth::AuthHandler`] traits are
the extension point a future helper crate can implement it against. SOCKS4/4a is
also out of scope (this is a v5-only library).

# Dual-use design

These types are **compliant by default, but deliberately violatable**. The
guided path ([`Client`](client::Client), [`Server`](server::Server)) emits and
accepts RFC-correct traffic; builder fields stay `pub` so a caller can override
them, parsers accept representable-but-non-conformant values as `Custom(..)`
variants rather than rejecting them, and the [`client::raw`] surface writes
exactly the bytes it is handed. This makes the crate usable for fuzzing,
interop testing, and red-teaming as well as for being a correct proxy. See
[`client::raw::malformed`] for ready-made spec-violating frames.

# Logging

The crate is instrumented with the [`tracing`] facade — connection lifecycle,
method selection, command dispatch, relay byte counts, and authentication
outcomes. Install a [subscriber](https://docs.rs/tracing-subscriber) to see
output; with none installed the cost is a single cached atomic check per call
site. To compile the instrumentation out entirely, depend on tracing with
`features = ["release_max_level_off"]` in your own manifest.

[RFC 1928]: https://www.rfc-editor.org/rfc/rfc1928
[RFC 1929]: https://www.rfc-editor.org/rfc/rfc1929
[RFC 1961]: https://www.rfc-editor.org/rfc/rfc1961
[`tracing`]: https://docs.rs/tracing
*/

pub mod auth;
pub mod client;
pub mod error;
pub mod server;
pub mod v5;
