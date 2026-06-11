/*!
A from-scratch implementation of the **SOCKS proxy protocol** — the protocol
that lets a client ask an intermediary to open TCP connections (CONNECT, BIND)
or relay UDP datagrams (UDP ASSOCIATE) on its behalf. All three wire versions
are implemented, each behind its own Cargo feature so you compile in only what
you use:

| Version | Feature | Targets | Auth | Modules |
|---|---|---|---|---|
| **SOCKS5** (RFC 1928/1929) | `v5` *(default)* | IPv4, IPv6, domain | method negotiation, user/pass | [`v5`], [`auth`], `client`/`server` |
| **SOCKS4** (1992 memo) | `v4` | IPv4 | userid field | [`v4`], `client::v4`/`server::v4` |
| **SOCKS4A** | `v4a` (⊃ `v4`) | IPv4 + domain | userid field | adds `connect_domain` / domain resolution |

```toml
socks = "0.1"                                                         # v5 (default)
socks = { version = "0.1", features = ["v4a"] }                      # all three
socks = { version = "0.1", default-features = false, features = ["v4"] }  # SOCKS4 only
```

Within an enabled version the crate is built in three layers, each usable on its
own: a high-level client/server, a mid-level pluggable auth (v5), and a
low-level codec for every wire message.

# Quick start (SOCKS5)

Connect to a host *through* a SOCKS5 proxy and speak to it as if the stream were
direct — the returned [`TcpStream`](std::net::TcpStream) is transparent:

```no_run
# #[cfg(feature = "v5")] {
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
# }
```

Serving is the mirror image — see [`server::Server`]. Username/password
authentication (RFC 1929) is one call away with
[`Client::with_user_pass`](client::Client::with_user_pass).

# Quick start (SOCKS4 / 4A)

SOCKS4 has no method negotiation: the client sends one request carrying an
optional `userid` and, in 4A, a destination domain name the *proxy* resolves.
Per-version types keep each version's surface honest — `client::v4::Client`
exposes only what SOCKS4 can express (IPv4 targets, no UDP):

```no_run
# #[cfg(feature = "v4")] {
use std::net::Ipv4Addr;
use socks::client::v4::Client;

# fn main() -> Result<(), socks::error::SocksError> {
// SOCKS4: connect by IPv4, identifying as "alice".
let stream = Client::new("127.0.0.1:1080".parse()?)
    .userid("alice")
    .connect((Ipv4Addr::new(93, 184, 216, 34), 80))?;
# let _ = stream;
# Ok(())
# }
# }
```

```no_run
# #[cfg(feature = "v4a")] {
use socks::client::v4::Client;

# fn main() -> Result<(), socks::error::SocksError> {
// SOCKS4A: let the proxy resolve the name (the `v4a` feature).
let stream = Client::new("127.0.0.1:1080".parse()?)
    .connect_domain("example.com", 80)?;
# let _ = stream;
# Ok(())
# }
# }
```

# Standards

Requirement-level provenance is maintained out-of-band in the workspace's
`protoref` compliance ledger; the prose here cites the source sections so the
behavior is traceable from the docs alone.

| Source | Title | Status in this crate |
|---|---|---|
| [RFC 1928] | SOCKS Protocol Version 5 | Implemented behind `v5` (CONNECT, BIND, UDP ASSOCIATE) |
| [RFC 1929] | Username/Password Authentication for SOCKS V5 | Implemented behind `v5` ([`auth`]) |
| [RFC 1961] | GSS-API Authentication Method for SOCKS V5 | Not implemented — see below |
| [SOCKS4] | SOCKS: A protocol for TCP proxy across firewalls | Implemented behind `v4` (CONNECT, BIND) |
| [SOCKS4A] | SOCKS 4A: A Simple Extension to SOCKS 4 Protocol | Implemented behind `v4a` (domain targets) |

GSS-API is intentionally out of scope for this crate so it stays
dependency-light; the [`auth::Authenticator`] / [`auth::AuthHandler`] traits are
the extension point a future helper crate can implement it against.

# Dual-use design

These types are **compliant by default, but deliberately violatable**. The
guided path emits and accepts spec-correct traffic; builder fields stay `pub` so
a caller can override them, parsers accept representable-but-non-conformant
values as `Custom(..)` variants rather than rejecting them, and each version's
`raw` surface writes exactly the bytes it is handed. This makes the crate usable
for fuzzing, interop testing, and red-teaming as well as for being a correct
proxy. See `client::raw::malformed` for ready-made spec-violating frames.

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
[SOCKS4]: https://www.openssh.com/txt/socks4.protocol
[SOCKS4A]: https://www.openssh.com/txt/socks4a.protocol
[`tracing`]: https://docs.rs/tracing
*/

#[cfg(feature = "v5")]
pub mod auth;
#[cfg(any(feature = "v4", feature = "v5"))]
pub mod client;
pub mod error;
#[cfg(any(feature = "v4", feature = "v5"))]
pub mod server;
#[cfg(feature = "v4")]
pub mod v4;
#[cfg(feature = "v5")]
pub mod v5;
