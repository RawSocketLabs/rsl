# session/socks

SOCKS wire codecs, beginning with SOCKS5 (RFC 1928 and RFC 1929). refcheck protocol name:
**`socks`**.

> Canonical agent-guidance file. The workspace root [`AGENTS.md`](../../../AGENTS.md) and
> protocol guide [`../../AGENTS.md`](../../AGENTS.md) also apply.

## Status

The default feature set provides pure SOCKS5 wire codecs: method negotiation, command and
reply registries, IPv4/domain/IPv6 endpoints, requests, replies, and RFC 1929 username/password
messages plus transport-independent client configuration and validation. Optional `blocking`,
`tokio`, and `mio` features add CONNECT operations, embeddable server
handshakes, complete per-connection proxies, and bounded listeners. Every client, server,
and proxy path is CONNECT-only; servers reply command not supported to BIND and
UDP ASSOCIATE. GSS-API, SOCKS4, and SOCKS4A remain absent.
[`DESIGN.md`](DESIGN.md) records decisions and limits.

## Architecture

- Group imports under `// --- Standard library ---`, `// --- Workspace dependencies ---`, and
  `// --- Internal modules ---`, in that order, omitting empty groups. The dependency group
  means workspace-managed crates (including bnb), not an `rsl_deps` facade. Keep
  attributes attached to their imports and alphabetize within each group. Apply the
  same grouping to scoped imports; facade re-exports remain after module declarations.
- Keep `lib.rs` and every `mod.rs` facade-only: documentation/attributes, module
  declarations, then re-exports. Alphabetize declarations and re-exports within their
  groups; backend order is `blocking`, `mio`, `tokio`. Types, constants, functions, and
  implementations belong in purpose-named files, even when small.
- `src/v5/wire/` owns the RFC 1928 and RFC 1929 wire types; `v5` re-exports those stable
  wire names. Wire codecs perform no I/O and select no runtime. `v5/client/` and `v5/server/`
  contain version-specific exchanges, with `blocking`, `mio`, and `tokio` siblings.
  `v5/client/config.rs` owns the re-exported `Config`; `v5/wire/version.rs` owns `VERSION`.
- Wire constants remain stored and public. Builders default them correctly; verbatim encoding
  preserves deviations and canonical encoding repairs them.
- `Endpoint` owns `ATYP`, address bytes, and port so compliant typed construction cannot create
  the draft's mismatched `address_type`/address pair. `Endpoint::Domain(Domain)` wraps the
  payload in `v5/wire/domain.rs`; `Domain` owns name/port, its bnb builder, and the typed
  length check. `Endpoint::validate_structure` only dispatches, without transport feature gates.
  `Endpoint::domain` and domain/request/reply builders enforce the 1–255-byte domain rule;
  `Endpoint::domain_raw`, raw fields, conversions, and decoding remain permissive. Do not
  add DNS/UTF-8 policy to these checks or treat a built value as immutable after construction.
  `Endpoint::validate_destination` adds a nonzero-port requirement only for guided
  CONNECT destinations, before client I/O and server request handoff. General endpoints,
  wire builders, and reply bound ports still allow zero.
- Explicit crate-private methods on the wire types own message-specific guided checks and
  reply construction; they are not parser hooks. The independent construction hooks on
  `Request`/`Reply` check only their endpoint, not CONNECT support or reply success.
  `v5/wire/validation.rs` contains only shared scalar checks. `v5/server/validation.rs`
  owns CONNECT-only capability enforcement, keeping it separate from wire validity.
  `src/error.rs` converts bnb's type-attributed `Endpoint` dispatch misses into
  `UnsupportedAddressType`; no header probe, rewind, or message-specific read wrapper is
  needed. Other codec errors retain their original details. `v5/auth.rs` owns
  authentication adaptation. There is no synthetic `Session` type.
  `src/server/policy.rs` owns ONE version-independent authentication/authorization policy. A boolean
  credential verifier establishes an authentication outcome, not an identity/principal.
  Never turn a claimed identity into verified authentication. `Destination` preserves domain
  bytes without a wire discriminant; policy sees protocol, authentication, operation, original
  destination, peer, and each exact resolved numeric target.
- Root `Client` selects a typed version-specific configuration. Root `Server` requires an
  explicit nonempty accepted-version set and a shared `Policy`; neither silently defaults.
  Only `Version::V5` exists. Do not add selectable placeholders or fallback negotiation.
  Future multi-version dispatch must retain the initial version byte in the same buffer.
  Unsupported greeting versions close without a guessed SOCKS5 reply.
- `src/client/mod.rs` is a facade: documentation, module declarations, and re-exports only.
  `client/client.rs`, `client/builder.rs`, and `client/configuration.rs` own `Client`, `Builder`,
  and `Configuration`; public `client/{blocking,tokio,mio}/` folders own distinct clients
  and specialized builder completion, with facade-only `mod.rs` files. The root `Client` remains
  transport-independent settings and the builder entry point; version-specific exchanges stay
  under `v5/client/`. `Client::configure(configuration)` requires settings up front; there
  is no default builder or missing-configuration state.
  Client configuration, V5 authentication choices, and construction errors remain available
  without transport features. Only backend-specific builders expose `build()`; the shared
  builder validates settings internally via `into_settings()`. Default normal dependencies
  must not enable bnb net, Tokio, or Mio.
  `client::Builder<B = Unselected>` owns shared options once; `client/backend.rs` owns
  re-exported, feature-gated backend markers. Only the unselected state can select a
  backend. Backend builder paths are aliases with specialized `build()` implementations,
  not wrapper structs or a public backend trait.
  Client configuration exposes `auth()`. Prefer `cfg` and `dest` for abbreviated local
  configuration and destination names; retain descriptive type and field names.
  Import referenced crate items at module scope instead of spelling `crate::` paths
  inside functions; use import aliases where names would collide.
  Select `.blocking()`, `.tokio()`, or `.mio()` before `.build()` for a concrete backend.
  Blocking/Tokio clients expose `connect` and `connect_with` (Tokio methods are async).
  Successful configured clients return `client::Connection<S>` from `client/connection/`;
  it delegates application I/O through `Stream`, exposes `bound()`, and preserves the
  buffer in `into_parts()`. Mio uses `Handshake::take_connection()` or returns the original
  poll alongside the connection from `connect_blocking`. This one-sided client connection
  is distinct from the root/server two-sided relay `Connection<S>`.
  Mio `connect_with` returns a caller-driven `Handshake`; `connect_blocking` explicitly
  drives its own poll loop. Preserve all readiness/fairness and single-handoff semantics.
  Supplied transports must already be connected and retain caller-owned deadlines;
  the configured budget applies to self-driving TCP setup only.
- `src/io/stream/` owns a bounded bnb incremental buffer and lossless raw-I/O handoff. Its
  facade re-exports `Stream` from `stream.rs`; `std.rs` owns standard-I/O implementations
  shared by blocking and Mio, and `tokio.rs` owns Tokio implementations. All driver
  handoffs retain `Stream<S>`, directly or inside a connection; never extract only the
  transport while buffered input remains.
  Its crate-private `read_message` / `read_message_async` methods own sequential-driver reads;
  the bounded Mio adapter borrows the same bnb reader through its fairness budget.
  Scratch stays method-local: retaining it enlarged the wrapper and showed no consistent
  benchmark benefit. It is not an additional queue of unread input.
- Blocking/Tokio version-specific exchanges delegate whole-message reads to bnb 0.6's
  borrowed, hint-aware readers, keeping
  unread application bytes in `Stream`. `blocking` enables `bnb/net`; `tokio` enables
  `bnb/tokio-io`, not `bnb/tokio` or `tokio-util`. Crate-private `Stream::write_message`
  and `write_message_async` own encoding, complete writes, and explicit flushing;
  the standard/Tokio write traits remain raw-byte I/O.
  Preserve original I/O sources and codec classification when converting reader errors.
  Blocking client/server folders have facade-only `mod.rs`, CONNECT stages, and
  shared command/authentication exchanges. Never send a fallback after partial success.
  Policy callbacks must be fast/nonblocking, not preemptible.
- `src/server/mod.rs` re-exports `Server`, its builder, and the two-sided `Connection<S>`
  from `server/server.rs`, `server/builder.rs`, and `server/connection/` respectively;
  `server/config.rs` owns `ServerConfig` and `Limits`, re-exported from `server`.
  `server::Builder<B = Unselected>` requires feature-gated backend selection before build.
  `server/{blocking,tokio,mio}/` own distinct servers and specialized builder completion;
  blocking/Tokio `exchange.rs` files own managed stages. The root `Server` is an
  uninhabited configuration entry point, not a backend-neutral runtime value.
  `src/proxy/` owns complete proxies,
  listeners, resolution workers, relay, and shutdown; this infrastructure is not copied
  beneath each protocol version. `src/io/` owns shared transport/deadline mechanics.
  `server/resolve.rs` owns shared domain-name preparation for system resolution, not wire
  validation. `io/deadline.rs` owns standard-clock deadline arithmetic and timeout-error
  helpers. Blocking and Mio use the same explicit-instant arithmetic
  through backend adapters; Tokio retains its own clock. These helpers remain crate-private.
  Blocking builders additionally expose `thread_budget` (default 128, minimum two).
  Blocking listener admission reserves two thread slots per session and takes the smaller
  of half this budget and `limits.connections`. This is per listener, excluding its caller
  and application-created threads; direct exchanges/`serve_connection` are not bounded by it.
  Per-connection blocking/Tokio drivers take their backend's `&Server`, not `&ServerConfig`;
  Tokio and Mio managed proxies own their `Server` configuration. Validate at server construction,
  not once per accepted connection. Selected blocking servers expose `serve` as a
  delegation to their proxy driver; Mio exposes consuming `into_proxy`, preserving caller-
  driven polling. Blocking and Mio entry points remain unchanged.
  Tokio additionally exposes infallible `into_proxy`, then fallible explicit `start`.
  `proxy/tokio/` owns `Proxy`, `Running`, and `Shutdown`; Tokio's public
  callback-based `serve` APIs are removed. The listener takes a concrete reporter and
  shutdown receiver, unwrapping task results in its select branch. Reporting is bounded to 64 errors and
  never backpressures sessions. `next_session_error` receives retained errors;
  `take_dropped_count` atomically retrieves/resets losses independently, including after
  EOF or join. Keep sampling cadence in application code; the guide covers batches and timers.
  `Running::join` is cancellation-safe and authoritative for service status/cleanup;
  shutdown wins simultaneously ready failures and discards cleanup completions.
  drop requests shutdown without awaiting it. Diagnostic EOF is not successful termination.
  `server/connection/` owns the established pair and blocking/Tokio relay impls; `proxy/blocking/` owns listener,
  worker, and cancellation sockets.
  Blocking `Server::exchange` and Tokio's async `Server::exchange` return driver-specific `Exchange` stages; consuming
  `authorize` retains permitted numeric addresses and an absolute deadline, and consuming
  `connect` returns both sockets only after the success reply. Full proxies compose these
  same stages. Keep blocking listener target registration before any success bytes.
  Blocking `Authorized::dial` returns `Connected`, which owns the unacknowledged target.
  The worker registers `Connected::target` under its cancellation mutex, releases the guard,
  then calls consuming `send_success`. On registration failure, consuming `fail(error)` closes
  the target before the bounded failure reply; reply errors take precedence over the cause.
  Public `connect()` composes `dial()?.send_success()?`; do not reintroduce callback-based
  `connect_with` or `serve_connection_with` in this path.
- Generic embedded `exchange` returns `Request<S>` with `send_success` / `send_failure`;
  `Exchange::into_request` opts into the same manual path with caller-managed deadlines.
  Never log credentials, silently allow destinations, or make no-auth an implicit fallback.
  Never re-resolve between authorization and connect, reset the dial budget at the stage
  boundary, or attempt a failure reply after writing any part of a success reply.
- Root `Client`, `Server`, `Connection`, `Stream`, `Destination`, and `Version` remain the
  entry types. Private `src/types/` owns the shared vocabulary in `destination.rs` and
  `version.rs`, re-exported through its facade and the crate root. Keep that folder limited
  to shared domain types, not general helpers. `src/error.rs` remains the guided API's
  shared error boundary; V5-specific error variants need an explicit design decision when
  adding V4, not a file move now. Policy is `server::policy` and resource limits are
  `server::Limits`, with no parallel root-module compatibility facade.

- `v5/client/mio/` and `v5/server/mio.rs` provide resumable exchanges. The client facade
  re-exports `Client` from `client.rs` and `connect_tcp` from the separate `connect.rs`
  convenience poll loop. `proxy/mio/` owns the complete
  proxy; `io/mio::Connector` owns numeric TCP connection mechanics.
  `proxy/mio/proxy.rs` owns the poll/listener/token loop, `entry.rs` owns per-connection state,
  and `shutdown.rs` owns its shutdown handle. `relay.rs` and `resolver.rs` retain their
  existing responsibilities. `Entry` owns construction, private protocol state, and
  phase-checked resolution delivery; `Proxy` owns polling/registration/scheduling.
  Cross-file internals stay scoped to `proxy::mio`. Reuse `Destination::socket_addr`
  for numeric destinations rather than another resolver-specific conversion.
  `v5/mio_io.rs` is the private version-specific bounded codec adapter, not generic I/O.
  Share strict policies/codecs/handoff with the other drivers; do not duplicate wire framing.
  Every partial write retains its offset. `WouldBlock` is pending; every other error poisons
  the handshake. A 64-operation fairness yield requires immediate continuation, not another
  readiness edge. Interests can be absent while awaiting policy or after completion.
- Keep Mio sockets associated with their original poll for their lifetime. The convenience
  client returns that poll alongside its stream. Check `take_error` and `peer_addr` after
  writable connect readiness, including platform-specific in-progress errors at MSRV 1.85.
- The Mio proxy owns non-reused tokens, bounded resolver channels/two workers, pinned target
  addresses, absolute deadlines, and two 16 KiB relay buffers per established connection.
  Preserve half-close, queued bytes under backpressure, callback-once behavior, stale-event
  rejection, and prompt shutdown. In-flight system DNS cannot be cancelled or safely joined.

## Testing

- `unit` tests live beside pure registry and helper logic.
- `tests/contract.rs` contains byte-exact RFC-derived message shapes.
- `tests/adversarial.rs` covers truncation, unsupported address types, invalid constants,
  unassigned codes, lengths, and parser no-panic behavior.
- `tests/api.rs` is the fast-tier explicit-configuration and neutral-destination contract.
  Driver suites cover every unsupported greeting version and configured client transcripts;
  the all-feature Mio suite checks the same shared policy across all three complete backends.

- `tests/session_blocking.rs` and `tests/session_async.rs` exercise RFC transcripts, hostile
  peers, and loopback composition. Transcript tests are the fast tier; loopback, fuzz, and
  mutation checks are the full tier. Loopback is confined to ephemeral local listeners;
  there are no third-party service dependencies or sleeps in the tests.
- `fuzz/` fuzzes client/server input through the bnb incremental decoder, bounded reads,
  and byte-exact handoff across fragmentation and coalescing.
- `benches/session_reads.rs` measures public handshakes under fragmented/coalesced in-memory
  reads (full tier); run `cargo bench -p socks --all-features --bench session_reads`.
  The bnb 0.6 adoption has a measured, accepted helper-path overhead (under 1 µs per
  handshake); see `DESIGN.md` before claiming a speedup.

### Routine local iteration

Follow the root guide's routine tier. For prose-only changes, review the diff and run
`git diff --check`; no Rust build or fuzz run is needed. Questions and discussion trigger
no checks. For Rust edits, run formatting, the affected behavior's tests, and strict
all-target SOCKS Clippy with the affected features. A single-driver edit normally needs
only that driver's configuration. Shared code or module moves normally use default and
all-features tests plus all-feature Clippy, not all eight combinations. Rebuild rustdoc
when changing documented API paths/examples. Keep targeted auth, denial, and lossless-handoff
regressions and independent review when relevant.

Do not run full workspace suites, feature/MSRV matrices, optimized suites, benchmarks,
mutation campaigns, or million-input fuzz runs after ordinary edits. Existing results for
unchanged code remain useful evidence; do not rerun them just to answer a question.

### Release qualification

During release preparation, or an explicit request for full qualification, run default,
`--features blocking`, `--features tokio`, `--features mio`, every pairwise driver combination,
and `--all-features` tests. Include strict Clippy, warnings-denied rustdoc and MSRV checks
across that matrix, optimized release tests, workspace checks from the root guide, detached
fuzz-workspace Clippy, mutation checks, and the session-read benchmark. Mio transcripts and
loopback tests live in `tests/session_mio.rs`; native client and complete proxy examples are
in `examples/mio_client.rs` and `examples/mio_proxy.rs` (required feature `mio`).

Run `cargo +nightly fuzz run --fuzz-dir protocols/session/socks/fuzz session
--target x86_64-unknown-linux-gnu -- -runs=2000000 -max_len=2048` and the same command with
target `mio_session`. These are release-tier local gates, not ordinary interaction gates.
Existing pre-push and hosted/scheduled CI requirements remain unchanged; do not bypass them.

## Scope notes

`#![forbid(unsafe_code)]` and `#![deny(missing_docs)]` are on. The default stays runtime-free.
Do not copy draft transport or
test infrastructure wholesale; recover behavior and evidence at the smallest owning layer.
