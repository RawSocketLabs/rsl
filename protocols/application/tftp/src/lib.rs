/*!
A from-scratch implementation of the **Trivial File Transfer Protocol** (TFTP) —
the minimal lock-step file-transfer protocol that runs over UDP, defined by
RFC 1350 and extended with option negotiation by RFC 2347/2348/2349.

The crate is built in three layers, each usable on its own:

| Layer | What it gives you | Start at |
|---|---|---|
| **High-level** | Transfer whole files: download (RRQ) and upload (WRQ) | [`client::Client`], [`server::Server`] |
| **Mid-level** | Send/receive individual packets with no state machine | [`client::RawClient`] |
| **Low-level** | Encode/decode every TFTP packet | [`wire`] |

# Quick start — download a file (RRQ)

```no_run
use tftp::client::Client;

# fn main() -> Result<(), tftp::error::TftpError> {
// Fetch "boot.img" from a TFTP server on the standard port 69, as raw octets.
let bytes = Client::new("203.0.113.10:69".parse()?).get("boot.img")?;
println!("downloaded {} bytes", bytes.len());
# Ok(())
# }
```

# Quick start — upload a file (WRQ)

```no_run
use tftp::client::Client;

# fn main() -> Result<(), tftp::error::TftpError> {
Client::new("203.0.113.10:69".parse()?).put("report.txt", b"hello tftp")?;
# Ok(())
# }
```

Serving is the mirror image — see [`server::Server`], which is driven by a
[`Handler`](server::Handler) that decides how read and write requests are
satisfied (an in-memory store ships as [`server::MemoryStore`]).

# Transfer modes & options

`octet` mode (the default) moves bytes verbatim; `netascii` mode applies the
CR-LF line-ending translation in [`netascii`]. With the `options` feature (on by
default) the client can negotiate `blksize`, `timeout`, and `tsize`
(RFC 2348/2349) and the server answers with an OACK.

# Standards

Requirement-level provenance is maintained out-of-band in the workspace's
`refcheck` compliance ledger; the prose here cites the RFC sections so the
behavior is traceable from the docs alone.

| RFC | Title | Status |
|---|---|---|
| [RFC 1350] | The TFTP Protocol (Revision 2) | Implemented (RRQ, WRQ, DATA, ACK, ERROR) |
| [RFC 2347] | TFTP Option Extension | Implemented behind `options` (OACK) |
| [RFC 2348] | TFTP Blocksize Option | Implemented behind `options` (`blksize`) |
| [RFC 2349] | TFTP Timeout Interval and Transfer Size Options | Implemented behind `options` (`timeout`, `tsize`) |

# Dual-use design

These types are **compliant by default, but deliberately violatable**. The
guided path ([`Client`](client::Client), [`Server`](server::Server)) emits and
accepts spec-correct traffic; wire fields stay `pub`, the codec preserves
unassigned codes as `Custom(..)` and imposes no length limits, and the
[`RawClient`](client::RawClient) surface sends exactly the packets it is handed.
This makes the crate usable for fuzzing, interop testing, and red-teaming as well
as for moving files. See [`client::raw::malformed`] for ready-made
spec-violating packets.

# Logging

The crate is instrumented with the [`tracing`] facade — transfer lifecycle, TID
selection, per-block progress, retransmissions, and option negotiation. Install
a [subscriber](https://docs.rs/tracing-subscriber) to see output; with none
installed the cost is a single cached atomic check per call site. To compile the
instrumentation out entirely, depend on tracing with
`features = ["release_max_level_off"]` in your own manifest.

[RFC 1350]: https://www.rfc-editor.org/rfc/rfc1350
[RFC 2347]: https://www.rfc-editor.org/rfc/rfc2347
[RFC 2348]: https://www.rfc-editor.org/rfc/rfc2348
[RFC 2349]: https://www.rfc-editor.org/rfc/rfc2349
[`tracing`]: https://docs.rs/tracing
*/

pub mod client;
pub mod error;
pub mod netascii;
pub mod server;
mod transfer;
pub mod wire;

pub use transfer::{BLOCK_SIZE, DEFAULT_MAX_RETRIES, DEFAULT_TIMEOUT, MAX_BLOCK_SIZE};
