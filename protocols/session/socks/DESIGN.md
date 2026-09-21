# SOCKS integration note

**Status:** SOCKS5 wire codecs plus blocking/Tokio/Mio CONNECT clients and servers implemented.
Source audit:
`origin/draft/protocols-socks` through `b93c3b9`; imported history through `c5df41a`.

## Current state

The draft contains SOCKS4/4A/5 codecs, authentication, clients, servers, CONNECT, BIND, UDP
ASSOCIATE, tests, and benchmarks. Its behavioral history is valuable, especially the complete
method/CMD/REP/ATYP registry checks, byte vectors, malformed-client cases, handshake/BIND
timeouts, and UDP source-pinning regressions. It is excluded and uses the predecessor
`binrw`/`derive_builder`/`testutil`/`refcheck` architecture.

This crate currently implements the RFC 1928 SOCKS5 wire seam—method negotiation, registries,
typed endpoints, requests, and replies—plus the RFC 1929 username/password request and response
messages on `bnb`. Optional blocking, Tokio, and Mio session layers now compose these codecs into
CONNECT clients, embedded server handshakes, per-connection proxies, and bounded listeners.
The current local trial delegates the drivers' hinted reads to bnb 0.6.0; the dated records
below retain the earlier framing decisions and measurements for comparison.

The original audit used `refactor/netlink-protocol-location`, three commits ahead of and four
behind `origin/main` at that point. Its unrelated untracked image assets were left untouched.
The current delivery candidate is main-based; see the delivery section below.

## Current structure

This is the authoritative source-ownership map. The dated records below describe earlier
states and retain superseded paths and verification results as history, not current guidance.
Every `mod.rs` and `lib.rs` is a facade containing documentation, declarations, and re-exports.

```text
src/
  lib.rs                       public entry points and feature gates
  error.rs                     shared guided-API failures
  client/                      cross-version configured client
    mod.rs
    builder.rs
    client.rs
    protocol.rs                typed version/configuration selection
    blocking.rs
    mio.rs
    tokio.rs
  server/                      configured server and shared policy
    mod.rs
    builder.rs
    config.rs                  ServerConfig and Limits
    connection.rs              established two-sided connection
    policy.rs                  authentication requirements, request facts, authorization
    server.rs
    blocking.rs                managed exchange/authorize/connect stages
    tokio.rs
  proxy/                       complete listeners, relay, and shutdown
    mod.rs
    blocking/
      mod.rs
      listener.rs              listener and complete per-connection handling
      relay.rs                 half-close-aware blocking relay
    mio/
      mod.rs
      entry.rs                 private protocol state and connection lifecycle
      proxy.rs                 poll/listener/token management and scheduling
      relay.rs
      resolver.rs
      shutdown.rs
    tokio.rs
  io/                          shared transport mechanics, not negotiation
    mod.rs
    address.rs
    deadline.rs                standard-clock arithmetic and timeout errors
    blocking.rs                blocking deadline adapter and message writes
    mio/
      mod.rs
      connector.rs
      deadline.rs              Mio deadline/error adapter
    stream/
      mod.rs
      stream.rs                bounded buffer ownership and lossless handoff
      std.rs                   standard I/O for blocking and Mio
      tokio.rs                 Tokio I/O
    tokio.rs                   Tokio timeout wrappers and message writes
  types/
    mod.rs
    destination.rs
    version.rs
  v5/                          version-specific protocol behavior
    mod.rs
    auth.rs
    mio_io.rs                  bounded, resumable V5 codec adapter
    client/
      mod.rs
      config.rs
      blocking.rs
      mio/
        mod.rs
        client.rs              resumable handshake
        connect.rs             convenience-owned poll loop
      tokio.rs
    server/
      mod.rs
      blocking.rs
      mio.rs
      tokio.rs
      validation.rs            shared CONNECT-only server capability checks
    wire/                      permissive RFC 1928/1929 bnb codecs
      mod.rs
      codes.rs
      domain.rs                domain payload, builder, and typed length validation
      endpoint.rs
      method.rs
      request.rs
      username_password.rs
      validation.rs            scalar checks used by explicit message methods
      version.rs
```

`Server::builder().build()` produces the validated server used by every complete backend:
blocking/Tokio `proxy::{blocking,tokio}::{serve,serve_connection}` borrow `&Server`, and
`proxy::mio::Proxy::new` takes ownership. `ServerConfig` remains an alternative construction
input through `Server::new(config)`, not the complete proxy entry-point argument. Embedded
version-specific exchanges still leave authorization and dialing to their caller.

## Decisions

- Use `bnb` for the wire codec and the workspace's `thiserror` for semantic construction
  diagnostics. The default wire surface needs no transport or runtime dependency; optional
  `blocking`, `tokio`, and `mio` features add the reviewed CONNECT behavior described below.
- Name wire messages by their role (`MethodRequest`, `MethodSelection`, `ReplyCode`) instead of
  preserving the draft's ambiguous `Identifier`, `Offer`, and `Response` API.
- Make `Endpoint` own `ATYP + address + port`. The draft stored `address_type`, address, and port
  separately, allowing ordinary typed construction to disagree.
- Preserve domain bytes rather than require UTF-8. The one-octet length is derived and checked.
- `Domain` owns the name/port payload and its 1–255-byte construction rule. Its bnb builder
  and the enclosing request/reply builders check that rule, as does `Endpoint::domain`.
  `Endpoint::domain_raw`, direct construction, conversions, and decode remain permissive.
  `Endpoint::Domain(Domain)` adds the address-type byte before that payload.
- Guided CONNECT destinations must use a nonzero port. This operation-specific check does
  not constrain wire construction/decoding or reply bound ports. Clients check before I/O;
  servers check before request handoff, resolution, authorization, or destination dialing.
- Preserve version and reserved bytes verbatim after decode. Builders supply RFC values;
  canonical encoding normalizes them.
- Preserve usernames and passwords as bytes rather than require UTF-8. Their one-octet lengths
  are derived and checked instead of retained as independently inconsistent fields.
- Reject empty credentials from compliant construction because RFC 1929 defines both lengths as
  1–255, while permissive decoding retains zero-length credentials from malformed peers.
- Model the status as a byte-backed newtype: zero is success and every nonzero byte is failure.
  This preserves every wire value without permitting a contradictory `Failure(0)` enum state;
  RFC 1929 does not assign distinct meanings to failure values.
- Represent construction failures as local semantic validation variants. `bnb` intentionally
  stringifies them into its public `BuilderError::Invalid` boundary.
- Reject empty method offers and the server-only `0xff` sentinel from compliant construction,
  while permissive decoding retains evidence from malformed peers.
- Keep unassigned method, command, reply, and standalone address-type values as `Other(u8)`.
- `bnb` builders now automatically normalize known-code `Other` aliases before validation:
  `Other(2)` becomes `UsernamePassword`, while unknown values stay unchanged. Thus `Other(0xff)`
  becomes `NoAcceptable` and is rejected in a method offer, but accepted in a server selection.
  Immutable validation normalizes a copy of each method before checking `NoAcceptable`, because
  raw construction and later mutation still permit aliases. Validation leaves the original
  variants and verbatim bytes untouched. No SOCKS-specific builder annotations or registry
  duplication are needed.
- Keep the default wire surface transport- and runtime-independent. I/O drivers are optional;
  feature-gating protocol versions is deferred until more than one version is implemented.
- Defer `rsl` facade exposure until the wire API survives review; workspace membership makes the
  crate testable without prematurely expanding the facade's compatibility surface.

## Unresolved questions

- Whether SOCKS4/4A should eventually be feature-gated or always compiled with the small codec.
- The future authentication extension boundary for asynchronous backends and authenticated
  principals; the current fast boolean predicate intentionally does not model either.

## Discarded approaches

- Merging or copying the draft: it would retain obsolete dependencies, duplicated wire state,
  broad feature gates, and client/server policy before the wire API is settled.
- Retaining advertised count fields in the typed messages: it makes ordinary values internally
  inconsistent. Deliberately false counts belong in the later raw/malformed surface.
- Retaining the draft's optional username/password length fields and stream readers: `bnb`
  directly expresses both messages without stored derived state or transport coupling.
- Adding shared `testutil`: no second SOCKS consumer exists; vectors stay local.

## Historical verification and implementation record

The initial results and dated entries below apply to their recorded source revisions. They
are not a claim that old blockers persist or that current local checks qualify a release.

- `cargo fmt --all --check`: passed.
- Focused RFC 1929 contract and adversarial tests: passed (28 tests).
- `cargo test -p socks`: passed (34 unit/contract/adversarial tests plus one doctest).
- `cargo clippy -p socks --all-targets --no-deps -- -D warnings`: passed.
- Independent staged-diff review: passed after closing an `AuthMethod::Other(0xff)` compliant-
  builder bypass and tightening two evidence/doc claims.
- Final cumulative staged-diff review, including the RFC 1929 status-newtype and typed method-
  request validation refactors: passed after eliminating an eager sentinel scan on oversized
  method lists; no findings remain.
- Follow-up review found no functional regression; its only concern was adding `thiserror`
  against the original no-new-dependency boundary, which the explicit follow-up direction
  superseded.
- `cargo test --workspace`: passed.
- `cargo +1.85.0 check --workspace --locked --offline`: passed at the workspace MSRV.
- `cargo clippy --workspace --all-targets`: passed with existing warn-level diagnostics.
- `cargo clippy -p socks --all-targets -- -D warnings`: blocked by 33 existing diagnostics in
  the local `bitsandbytes-macros` dependency; the SOCKS target itself is clean under denied
  warnings.
- `cargo deny --offline --locked check`: bans, licenses, and sources passed; the aggregate check
  failed because existing `p256`/`p384` development dependencies select yanked `wnaf 0.14.0`.
  Unmatched license allowances and the existing `syn` 2/3 duplicate were warnings, not failures.

All checks were local/offline; no service or third-party system was contacted.

### Upstream builder-normalization follow-up (2026-09-06)

- RFC 1928 §3 discriminants remain byte-identical: method offers `[5, 3, 0, 2, 0x80]` and server
  refusal `[5, 0xff]` now also have the same Rust variants whether built or decoded.
- SOCKS: 35 tests plus one doctest pass; direct/mutated `Other(0xff)` remains rejected by
  immutable validation and writable verbatim. Strict SOCKS-only Clippy (`--no-deps`) passes.
- bnb: nine focused macro tests pass; nested containers, defaults, validation timing, enum
  widths, opaque codec boundaries, canonical/verbatim separation, and all builder entry paths
  are covered. Four runtime mutation tests were caught; two million fuzz inputs passed.
- Independent final review approved after resolving bare-derive exclusion and helper-visibility
  findings; no findings remain.
- Workspace tests and normal Clippy, `bytes`/`mock`/`tokio` feature tests, formatting, MSRV
  production checks, bare-metal no_std, and the updated pinned public-API snapshot pass.
- Strict aggregate Clippy and cargo-deny retain the previously documented blockers. Additional
  existing bnb verification debt (runtime warnings, test-dependency MSRVs, unavailable semver
  tooling, rustdoc warnings) is recorded in `bitsandbytes/bnb/DESIGN.md`.

This adds `bnb::NormalizeEnumAliases` and builder behavior, not a SOCKS API or dependency.
Custom-codec/logical-only boundaries are intentionally opaque; arbitrary callbacks referencing
normalized siblings can observe the variant change. The protocol roadmap status is unchanged.

### Copy-only method validation (2026-09-07)

- Validation now normalizes each copied method, then checks only `NoAcceptable`. It retains
  length-error precedence and never rewrites the request or its verbatim encoding.
- The existing raw `Other(0xff)` rejection test and the new selectable-alias preservation test
  pass: 36 SOCKS tests plus one doctest. Formatting and strict all-target SOCKS Clippy pass.
- Workspace tests, configured workspace Clippy, and all four cargo-deny gates pass offline.
  Existing unrelated workspace warnings remain; the earlier bnb macro warnings and yanked
  wnaf blocker are resolved by the already-staged bnb release work, not this refactor.
- Independent review: no findings. No API, dependency, or roadmap-status change.

### Consuming normalization adoption (2026-09-07)

`bitsandbytes 0.4.1` adds `into_normalized_enum_aliases(self) -> Self`; macros remain
at `0.4.0`. PRs #73 and #74 were merged under `michael-smythe` after green CI and
independent review. The release workflow passed; crates.io records that publisher,
and the downloaded archive checksum and tag target match the released candidate.

Immutable method validation now compares each copied method's normalized value
directly with `NoAcceptable`. Length-error precedence, raw alias rejection,
selectable unknown methods, and original variants/bytes are unchanged. The helper
does not clone, mutate the request, validate credentials, or add protocol behavior.
At that adoption point, workspace bnb pins and local package metadata followed the published
versions; the original integration checkout retained path dependencies and unrelated staged
work rather than reconciling main. That adoption alone changed no SOCKS API, transport,
dependency, or roadmap status.

Local adoption verification: 36 SOCKS tests plus one doctest, eleven bnb normalization
tests, workspace tests, formatting, strict all-target SOCKS and all-feature bnb/macros
Clippy, configured workspace Clippy, and all four cargo-deny gates pass offline.
Rust 1.85 workspace/renamed-consumer and bare-metal checks pass; the pinned public API
matches and all/default/no-default patch comparisons against published 0.4.0 pass.
The historical macro warnings and yanked-wnaf failure remain resolved; unrelated
workspace warn-level lint debt remains outside this slice.
Independent final review passed after synchronizing the published changelog entries
and distinguishing isolated release scope from this checkout's retained configuration.
The next smallest slice remains the explicit raw/malformed codec surface below.

### Blocking and Tokio CONNECT sessions (2026-09-07)

The user's direction prioritizes both an embeddable server and a complete listening proxy,
blocking first and async second, ahead of the previously planned raw/malformed convenience API.
The raw wire types and their permissive decode/canonical/verbatim contracts are unchanged.

#### API and layering decisions

- No default runtime dependency. Feature `blocking` adds standard-I/O sessions; feature `tokio`
  adds the existing workspace Tokio dependency. Both can be enabled independently or together.
- Each driver exposes `connect` over a generic duplex stream, `connect_tcp` for bounded TCP
  setup, `accept` yielding an `Incoming` request, `serve_connection` for a complete proxy
  connection, and `serve` for a bounded listener with explicit shutdown.
- `Incoming::accept(bound)` sends success only when the embedding caller has authorized and
  connected the destination. `Incoming::reject(code)` cannot send a success code. Full proxies
  do the dial themselves and report the target connection's actual local endpoint.
- `ClientAuth` offers exactly one explicit method; username/password never falls back to
  no-auth. `ServerAuth` likewise selects only its configured method. RFC 1929 authentication
  validates the subnegotiation version and nonempty credentials before invoking the predicate;
  every nonzero status fails and no CONNECT request follows a rejection.
- `ServerConfig` requires an authorization predicate receiving peer address, original endpoint,
  and each exact resolved numeric address. Only permitted numeric addresses are dialed; there
  is no second hostname resolution between authorization and connect. Allowing every target
  is an explicit caller choice, not a default open proxy.
- Shared phase framing asks for only the next length/header boundary, then delegates complete
  decoding to bnb. Maximum frames are 257-byte greeting, 513-byte credentials, 262-byte command,
  and two-byte selections/status replies. It never consumes coalesced tunnel bytes.
  `bnb::MessageStream::into_inner` does not return buffered read-ahead; exact boundary reads
  avoid that handoff hazard without creating a general transport abstraction or changing bnb.
- Session validation rejects invalid versions, RSV, unsupported methods/commands/address types,
  and empty domains. Raw wire decoding remains deliberately permissive. Unknown ATYP receives
  address-type-not-supported after its header; the session is not reused.
- Errors use a shared non-exhaustive `thiserror` enum with preserved I/O/codec sources and no
  credential-bearing diagnostics. Local endpoint/credential validation precedes timeout
  validation and all network writes in both TCP client APIs.

#### Lifetimes, deadlines, and security limits

- Defaults: 10 seconds for the entire handshake, 10 seconds shared by outbound connection
  attempts, 300 seconds **total relay lifetime** (not idle timeout), and 64 active sessions.
  Zero/overflowing durations and zero capacity are invalid. Reply writes have a separate budget
  bounded by the configured handshake duration and ten seconds.
- Blocking socket operations use a remaining absolute deadline; a slow peer cannot renew the
  handshake by sending one byte per socket timeout. Tokio uses phase-wide timeouts. Generic
  stream APIs intentionally leave deadlines to their embedding caller.
- Blocking DNS is synchronous and outside the dial budget; it can delay shutdown. Tokio awaits
  DNS within the dial budget, but cancelling that wait cannot terminate a system resolver call
  already executing in its blocking pool. Resolver work is not a cancellable custom DNS API.
  System resolution requires nonempty UTF-8 domains without NUL; wire bytes are still opaque,
  and clients forward domain bytes without local DNS. No IDNA conversion is supplied.
- Both relays preserve TCP half-close: EOF closes the opposite write side while allowing its
  response to drain. Errors/deadlines terminate both directions. Blocking uses one session
  thread plus a scoped relay thread; Tokio uses one task with bidirectional copying.
- Listener capacity bounds accepted sessions; excess connections remain in the OS backlog.
  Shutdown stops acceptance, closes/aborts active sessions, and joins workers. Callers retain
  responsibility for backlog configuration and binding to appropriate interfaces.
- Cancellation/error is terminal. Owned transports are dropped; callers that supplied borrowed
  streams must close them, not resume a partially read handshake. The async listener aborts
  its owned session tasks on shutdown. Blocking DNS, caller callbacks, and an in-progress
  bounded connect attempt cannot be interrupted. Active blocking relays register both sockets
  in a listener-owned cancellation handle; late registration after shutdown refuses the target.
- Authentication and authorization predicates must be fast, nonblocking, and non-panicking.
  Slow password hashing and asynchronous credential stores need an explicit later API, not
  blocking work inside Tokio tasks. Authentication returns a boolean, not a principal;
  destination policy is per-peer/target, not per-authenticated-user. Password storage,
  constant-time verification, rate limiting, TLS, and credential zeroization remain caller
  responsibilities. Credentials have no `Debug` and are not logged.
- This is a SOCKS5 CONNECT subset, not a claim of complete RFC 1928 conformance: GSS-API, which
  RFC 1928 requires for compliant implementations, is not implemented. Neither are BIND,
  UDP ASSOCIATE, SOCKS4, or SOCKS4A.

#### Protocol evidence and verification

RFC 1928 §§3–6 supplies method selection, request/reply framing, CONNECT success/failure,
and BND endpoint semantics; RFC 1929 §2 supplies the authentication exchange and nonzero
failure rule. The locally available draft's silent-client timeout regression is recovered
as a bounded socket test, without its binrw architecture or sleep-based harness.
Transcript fixtures assert exact bytes, fragmentation, coalescing, no tunnel read-ahead,
authentication failures/downgrades, truncation, and strict-versus-raw validation. Loopback
tests cover target authorization before dial, half-close, bounded capacity, shutdown,
deadlines, and blocking-client/Tokio-server interoperability. Async duplex tests also force
pending I/O and cancellation. The phase parser has a standalone libFuzzer target.

Final local verification:

- Default wire suite: 36 tests and one doctest. All features: 63 tests (15 blocking,
  12 Tokio, 36 wire) and four doctests. Independent blocking-only/Tokio-only builds and
  tests pass; Cargo skips test binaries whose required feature is absent.
- Formatting, strict SOCKS Clippy for default/blocking/Tokio/all features, default/all-feature
  rustdoc with warnings denied, Rust 1.85 workspace and all-feature SOCKS checks pass.
- Workspace tests, configured workspace Clippy, and cargo-deny pass. Unrelated warn-level
  workspace diagnostics remain; historical bnb macro warnings and yanked-wnaf failures
  remain resolved by the earlier bnb release work.
- Two million ASan libFuzzer runs pass with `-max_len=2048`, exceeding the maximum handshake
  bound; valid negotiation prefixes also expose deep CONNECT framing to mutation.
- Three intentional mutations are caught: skipping credential validation before policy,
  reading one extra IPv4 frame byte, and omitting target shutdown. The first initially
  survived because a test callback independently rejected empty credentials; the new
  callback-boundary test closes that gap. All mutations were restored before final checks.
- Independent final review found and resolved a blocking relay shutdown defect: closing
  only the client could leave the target read waiting until the relay deadline. Listener
  ownership now covers both sockets and races with target registration. The dedicated
  regression keeps the target open and requires listener completion before the relay budget.
  Re-review finds no remaining code issues; bounded-connect shutdown latency is documented.
- CI now checks each session feature, cross-driver interoperability, strict SOCKS Clippy,
  rustdoc, and MSRV. Local actionlint 1.7.12 passed at the original implementation review;
  hosted CI had not yet been triggered there. Fuzzing remains a documented local full-tier check.

That implementation verification ran dependency/build checks offline; protocol references
were consulted at RFC Editor and Tokio documentation, and transport tests used ephemeral
loopback listeners. It did not publish or reconcile branches, or modify unrelated PNGs.

### Main-based delivery candidate

`feat/socks-connect` is based on main `67876220` (bnb 0.4.1). It carries the complete
reviewed SOCKS iteration, including the local `no_acceptable_methods` variable rename,
workspace membership, the lockfile entry, status documentation, and feature-specific CI.
It preserves main's containerized pre-push validation, bytemuck capability, IP/ARP fixes,
and bnb-only release hold. It does not reintroduce this integration branch's older release
configuration or include its unrelated crypto/netlink documentation commits or PNG assets.

Delivery requires the main-based local checks, independent review, complete containerized
pre-push validation, and hosted PR CI. No SOCKS registry publication is enabled by this change.

### bnb 0.5 incremental decoding and lossless handoff (2026-09-08)

This main-based follow-up adopts the published bnb 0.5 core without changing the wire API,
dependencies, authentication policy, or supported commands. It supersedes the original exact-
boundary framer described above. That framer protected the bare-transport return contract;
reading ahead while returning only `S` would have discarded application bytes.

#### API and ownership

- With the user's approval, both drivers' `connect`, `connect_tcp`, and `Incoming::accept`
  return `Tunnel<S>` / `Tunnel<TcpStream>` rather than a bare transport. The wrapper implements
  the corresponding standard/Tokio read and write traits. Existing application I/O continues
  through it; TCP-specific configuration uses `tunnel.get_ref()` or `get_mut()`.
- `try_into_inner` succeeds only when no unread bits remain; failure returns the unchanged
  wrapper. `into_parts` transfers both the transport and `BitBuf`, retaining its cursor and
  capacity; `from_parts` reconstructs without copying/resetting. There is no unconditional
  `into_inner`, `Deref`, `Clone`, or credential-exposing `Debug` implementation.
- Raw reads return available buffered bytes before touching the underlying stream, including
  when the stream would block or fail. Empty reads do no I/O. A partial-bit cursor supplied
  through `from_parts` produces `InvalidData` without consuming input; bit-level recovery stays
  explicit in bnb. Direct reads through transport accessors bypass the prefix and are documented
  as inappropriate for application I/O.
- The default feature set remains wire-only. Blocking and Tokio stay independently usable.
  bnb `MessageStream`/`BinCodec` remain valid general adapters, but adopting both would require
  distinct ownership wrappers and additional async dependencies here. The shared `BitBuf` core
  is the existing cross-driver seam, not a new SOCKS-specific framing engine.

#### Framing, bounds, and failures

- Each handshake owns one reusable `BitBuf::bounded(513)`. Both drivers first attempt `try_pull`,
  read at most the remaining capacity when incomplete, then append and retry. The old `Frame`
  enum and its duplicate length/header arithmetic are removed. No per-message receive `Vec`
  remains; the bounded I/O scratch array is 513 bytes. Typed credential/domain allocations and
  encoded-output allocations remain; this is not an allocation-free codec claim.
- After `Incomplete { needed: Some(n) }`, both drivers accumulate at least `n` additional bytes
  before replaying the decoder. This is the blocked operation's shortfall, not a complete-frame
  length. Unknown/zero hints require at least one new byte. Reads still use free buffer capacity,
  allowing coalescing; saturating subtraction handles receiving more than the hinted minimum.
- EOF immediately makes a finite decode attempt, even before the shortfall is satisfied. Every
  read retains the existing capacity/deadline checks; interrupted reads change neither the
  shortfall nor buffered input. No polling, peeking, sleeping, or SOCKS length arithmetic is added.
- Incomplete attempts remain transactional, not resumable within a message. Already complete
  fields can still be decoded/allocated again at the next field boundary; unknown hints can
  still cause retries after each read. Batching eliminates unnecessary attempts within a known
  shortfall, not all replay or allocation. The private drivers only decode built-in SOCKS wire
  types; this is not a scheduling guarantee for arbitrary custom codecs with speculative hints.
- RFC 1929 §2 sets the largest frame at `1 + 1 + 255 + 1 + 255 = 513` bytes. RFC 1928 §§3–6
  messages fit below that limit (greeting 257, command/reply 262, selections/status 2).
  Capacity is checked before consuming more transport bytes. bnb owns transactional retry and
  payload-length handling. A coalesced client transcript is measured to need one transport
  read; no broader throughput or latency improvement is claimed without benchmarks.
- Reads may include subsequent phases and early tunnel data, but strict validation and writes
  still occur in protocol order. Each successful decode consumes only its message. On EOF,
  `pull_eof` distinguishes a truncated frame (`Error::Codec` with finite-EOF details) from a
  missing frame (`Error::Io(UnexpectedEof)`). Original transport error sources are retained and
  `Interrupted` reads are retried. Explicit write flushing is unchanged.
- An unknown ATYP still gives `UnsupportedAddressType(code)` and server reply `0x08` after its
  four-byte header. On a command codec error, a bnb source probe reads the retained header and
  restores its cursor on every path. This only classifies errors; it computes no frame length
  and never matches diagnostic strings. Valid ATYP truncations remain codec errors.
- Successful handoff is lossless. Handshake errors and async cancellation remain terminal,
  including for borrowed transports: close them rather than attempting recovery. This slice
  deliberately adds no resumable negotiation/state-machine API.

#### Relay and remaining limits

- Blocking TCP setup unwraps its deadline adapter by transferring both parts, then clears TCP
  handshake timeouts. The full blocking relay gives the retained prefix only to its outgoing
  reader; both directions run concurrently under the same absolute relay deadline. It does
  not synchronously drain a prefix before starting the other direction. Target registration,
  listener cancellation, and half-close handling are unchanged.
- Tokio passes the wrapper directly to bidirectional copying under the existing total relay
  timeout. Pending I/O cannot hide already buffered data. Raw writes, flushes, and async
  shutdown delegate to the underlying transport.
- Buffers can retain consumed credential storage, including after extraction. No zeroization
  or secrecy wrapper is added. The receive cap bounds buffered input, not callback work, raw
  application traffic, or arbitrary buffers imported by `from_parts`. Existing DNS, callback,
  authorization, cancellation, and CONNECT-only limitations still apply.

#### Initial verification (before shortfall batching)

- Default wire suite: 36 tests plus one doctest. All features: 75 tests (21 blocking,
  18 Tokio, 36 wire) plus four doctests. Default/blocking/Tokio/all-feature suites and strict
  all-target Clippy pass independently. Formatting, warnings-denied all-feature rustdoc,
  Rust 1.85 workspace and all-feature SOCKS checks pass.
- Workspace tests and configured workspace Clippy pass. The latter retains 106 warning lines
  from unrelated existing workspace debt; strict SOCKS checks are clean. Offline cargo-deny
  passes advisories, bans, licenses, and sources. Historical bnb macro warnings and the
  yanked-wnaf blocker remain resolved by the already-delivered upstream release.
- RFC-derived transcripts prove exact output, fragmented/coalesced input, 255-byte credentials,
  bounded receive requests, every unknown ATYP, and valid-ATYP truncation. Handoff tests cover
  parts reconstruction, refused lossy extraction, alignment, EOF/source distinctions, and
  returning buffered data before a pending/failing transport. Both full proxies preserve an
  8 KiB pipelined payload followed by FIN and drain the target's response after half-close.
- Two million ASan libFuzzer cases pass with `-max_len=2048`. Arbitrary hostile inputs exercise
  both client/server drivers; fixed valid authentication/CONNECT prefixes plus arbitrary tails
  independently check byte-exact handoff over five read partitions and enforce the receive cap.
- All 20 selected wrapper mutations are caught. Two initially timed out in the full suite;
  both then fail assertions immediately in focused non-network handoff tests. Three additional
  deliberate mutations are caught: reducing capacity to 512 and discarding the prefix in each
  full relay. Every mutation was restored before the final feature/Clippy/workspace checks.
- Independent cumulative reviewer approval: no blocking source, API, test, fuzz, or scope
  findings. Documentation follow-ups record the retry-work limitation and these final results.
- Checks used cached dependencies/offline mode; transport tests used ephemeral loopback only.
  No external network calls, push, PR, merge, tag, or release occurred. Containerized pre-push
  and hosted CI are still delivery gates, not claimed here. The original integration checkout's
  130 staged files, unstaged method-variable rename, and three PNGs remain untouched.

The protocol roadmap stays at `dev`: this replaces transport plumbing, not the supported
protocol set or conformance status. No SOCKS release is enabled.

#### Shortfall-batching measurements and verification

`cargo bench -p socks --all-features --bench session_reads` compares maximum RFC 1929
credentials and maximum RFC 1928 domains under 1-byte, 16-byte, and coalesced reads. It uses
the public blocking/Tokio `accept` paths with in-memory input and discarded writes, optimized
builds, 100 warmups, and the median of five samples of 10,000 handshakes. No new dependency,
unsafe allocator hook, timing assertion, or network service is used.

Local characterization on 2026-09-08 (rustc 1.98.0), before/after batching, for one-byte reads:

| Handshake | Retry after every read | Shortfall batching |
|---|---:|---:|
| Blocking, maximum credentials | 12.681 µs | 2.683 µs |
| Tokio, maximum credentials | 12.781 µs | 2.997 µs |
| Blocking, maximum domain | 9.471 µs | 1.636 µs |
| Tokio, maximum domain | 11.148 µs | 1.710 µs |

The 16-byte cases also improved; no coalesced regression was observed in this run. This
isolates local session/codec overhead, not real network throughput or latency, and does not
count allocations. Results are evidence for this refinement, not portable performance limits.

- All-feature suite: 80 tests (23 blocking, 21 Tokio, 36 wire) plus four doctests. Independent
  default/blocking/Tokio/all-feature tests and strict all-target Clippy pass, including the
  benchmark build. Formatting, warnings-denied rustdoc, Rust 1.85 workspace/all-target SOCKS
  checks, workspace tests/configured Clippy, and offline cargo-deny pass. Existing unrelated
  workspace lint debt remains; the historical bnb macro and yanked-wnaf blockers stay resolved.
- Added EOF cases inside long username/password/address fields, interruptions and transport
  failures during a shortfall, and async Pending/resume. Existing maximum-frame, coalesced-tail,
  cap, half-close, and deadline tests remain green. The empty-buffer unknown hint is exercised
  by ordinary handshakes; the zero-hint fallback is defensive for future codecs, not a value
  currently emitted by the generated SOCKS fields.
- A fresh two-million-case ASan fuzz run passes with `-max_len=2048`. Six targeted automatic
  interruption/error-guard mutations are caught. Two deliberate accounting mutations (growing
  the shortfall and using non-saturating subtraction), applied to both drivers, each fail both
  maximum-credential tests. All mutations are restored before the final verification rerun.
- Independent review identified a doubled-escape test-fixture typo; both fixtures were corrected
  and now exercise the intended valid-ATYP truncation path. No source finding remains.
- No public API, dependency version, wire behavior, authentication policy, or release setting
  changes. The benchmark reuses existing std/Tokio dependencies. Delivery gates and preservation
  of the original checkout remain as recorded above; only the review worktree is modified.

### bnb 0.6 borrowed-reader adoption (2026-09-14, local trial)

This supersedes the manual read loops and dependency-version statements in the dated 0.5
records above. The review branch was fast-forwarded to locally available main `9b29a2a8`,
retaining its 14-file staged patch byte-for-byte. Main supplies runtime `bitsandbytes 0.6.0`
and macros `0.5.0` through the existing workspace path dependencies. The old ignored fuzz lock
was backed up; main's tracked 0.6 lock was retained and only its bnb-to-thiserror edge changed.

#### Implementation and contracts

- `Tunnel::read_message` and `Tunnel::read_message_async` call `bnb::net::read_message` and
  `read_message_async`, respectively. They borrow the existing transport, bounded `BitBuf`,
  and 513-byte stack scratch; no additional ownership wrapper or per-read receive allocation
  is introduced. Obsolete SOCKS capacity, push, and EOF helpers are removed.
- Feature `blocking` forwards to `bnb/net`; `tokio` forwards to `bnb/tokio-io` and the existing
  optional Tokio dependency. Default wire-only builds remain runtime-free. Neither driver
  enables `tokio-util`; no new direct dependency or release setting is added.
- bnb owns shortfall batching, retained-capacity checks, interruption retry, finite decoding
  at EOF, and immediate replay when buffer compaction invalidates a positional hint. Positive
  hints remain additional-byte lower bounds, not frame lengths. Every SOCKS message is byte-
  aligned, so the helper's final-byte padding consumption changes no SOCKS wire boundary.
- `From<bnb::net::MessageReadError> for Error` preserves the original owned `Io` or `Codec`
  payload. It inherits the existing session-feature gate on the error module. The upstream
  enum is non-exhaustive: a future variant is retained as an `io::Error::other` source and
  classified as general failure, without guessing its semantics or matching strings.
- Typed codec errors still reach the retained-header probe: unknown ATYP produces server
  reply `0x08`; valid-ATYP truncation stays a codec error. Empty EOF remains `Io(UnexpectedEof)`.
  Three new unit tests prove codec kind/offset/field, owned transport source, and OS-code
  preservation. Existing transcript tests cover composition through both readers.
- The staged `Tunnel<S>` public API and lossless handoff are unchanged by this adoption.
  Authentication, destination authorization, deadlines, and terminal session-error/cancellation
  policy are unchanged. Retained input does not make SOCKS negotiation resumable.
- Encoding, `write_all`, and explicit flushing stay local: bnb's existing writer neither
  preserves the full owned transport error nor flushes. Credential/endpoint allocations,
  repeated partial-field decoding, non-zeroized storage, and CONNECT-only limits remain.

#### Performance finding

The existing in-memory benchmark was run before adoption, then in three alternating pairs
against an isolated control using the old SOCKS loops with the **same bnb 0.6.0**. Each entry
below is the median of those three benchmark medians (each 5 × 10,000 handshakes, 100 warmups,
rustc 1.98.0, default optimized bench profile). The original 0.5 baseline and same-version
control are comparable; the slowdown is associated with the borrowed-helper path.

| Input/read chunk | Blocking control → helper | Tokio control → helper |
|---|---:|---:|
| Max credentials / 1 byte | 2.785 → 3.544 µs | 3.073 → 3.857 µs |
| Max credentials / 16 bytes | 447 → 602 ns | 590 → 700 ns |
| Max credentials / coalesced | 253 → 328 ns | 398 → 437 ns |
| Max domain / 1 byte | 1.502 → 2.033 µs | 1.829 → 2.287 µs |
| Max domain / 16 bytes | 286 → 345 ns | 419 → 460 ns |
| Max domain / coalesced | 146 → 182 ns | 270 → 291 ns |

This run shows 8–35% higher local handshake overhead, less than one microsecond absolute.
Hardware instruction counts for the full benchmark increased from approximately 13.08 to
15.45 billion (18%). A cycle profile samples out-of-line `bnb::net::read_limit` and `shortfall`
at approximately 6% and 2%; retained-buffer `push` remains the largest sampled function.
These observations identify an upstream profiling target, not proof that inlining alone
fixes the regression. This is not a network-throughput, latency, or allocation-count result;
timing remains characterization, not a test assertion or portable performance guarantee.
No upstream bnb optimization is included in this local trial. Before delivery, either address
the helper overhead upstream or explicitly accept the measured tradeoff for shared behavior.

#### Verification and delivery boundary

- Default wire suite: 36 tests and one doctest; all features: 83 tests (36 wire, three error
  conversion, 23 blocking, 21 Tokio) and four doctests. Default/blocking/Tokio/all-feature
  tests, strict all-target Clippy, warnings-denied rustdoc, and Rust 1.85 all-target checks pass
  independently. Production dependency trees exclude Tokio in default/blocking mode and
  exclude `tokio-util` in every mode. Formatting passes.
- Workspace tests, configured all-target workspace Clippy, Rust 1.85 workspace checks, and
  offline cargo-deny pass. Existing unrelated workspace warn-level lint debt remains; the
  historical bnb-macro warnings and yanked-wnaf blocker remain resolved, not exemptions.
- Three fault mutations in an isolated copy are caught: discarding the buffered prefix by
  reporting it empty, mapping codec failures to I/O failures (both drivers' ATYP tests), and
  reducing transport errors to their kind (both owned-source and OS-code unit tests). The
  working checkout was never mutated for these checks.
- Two million ASan libFuzzer inputs pass with `-max_len=2048` (370 seconds). Existing RFC-derived
  valid prefixes, arbitrary tails, fragmentation, coalescing, and receive-bound checks remain
  active. Final all-feature tests and strict Clippy pass after the isolated fault checks.
- Independent cumulative source/API/test/fuzz review and documentation re-review report no
  findings. The measured performance regression remains an explicit delivery decision.
- Checks use cached dependencies/offline mode; transport tests use ephemeral loopback only.
  No external network, commit, push, PR, tag, or release is part of this trial. Containerized
  pre-push and hosted CI remain later delivery gates. The original integration checkout and
  its unrelated staged work, unstaged rename, and three PNGs remain untouched.

Roadmap status remains `dev`: this changes read plumbing, not the supported protocol set.
The immediate follow-up is the measured reader-overhead decision above. The next protocol
slice remains embedded blocking BIND, as described below.

#### Tunnel method placement and scratch experiment (2026-09-14)

The private driver functions became crate-private inherent `Tunnel` methods: `read_message`
under `blocking`, and `read_message_async` under `tokio`. Distinct names coexist when both
features are enabled and do not shadow raw `Read::read`. All twelve handshake call sites now
use these methods. This expands no public API and leaves strict session validation, typed
ATYP error classification, raw handoff, and bnb's read contracts unchanged.

A `[u8; 513]` array local to each message read has no heap allocation or destructor. Its
initialization is the potential saving from reuse, not an allocation/deallocation pair.
An async read retains that local in its future across suspension. Adding an inline field
would instead retain it with every `Tunnel` through application I/O and increase the size
of moved tunnels, including the lossless `try_into_inner` error value.

The inline-field experiment initialized scratch in `from_parts` and shared it across message
reads; bnb still immediately appended accepted bytes to `BitBuf`. Against the method-local
array, three alternating pairs of optimized benchmark runs showed no consistent improvement.
Maximum-credential cases ranged from 2% faster to 8% slower; maximum-domain cases were 2–27%
slower. Coalesced domain medians were 176 → 223 ns blocking and 300 → 381 ns Tokio. These are
same-host characterization medians, using the benchmark procedure above, not portable limits
or proof of the exact machine-code cause. The experiment also failed strict Clippy because
`try_into_inner`'s `Err` variant became at least 569 bytes. Boxing would add allocation or
change the API; suppressing the warning would not establish a performance benefit.

The retained-field experiment was removed. Scratch stays local to the inherent methods;
`Tunnel`'s representation and two-part handoff remain unchanged. No extra scratch-clearing,
secrecy, or zeroization guarantee is introduced. The earlier bnb helper-overhead finding
remains separate and unresolved. Existing transcript, fragmentation, maximum-length, EOF,
interruption, cancellation, and handoff tests exercise the relocated methods through public
session APIs; no tests pin method visibility, private fields, or scratch addresses.

Fresh default/blocking/Tokio/all-feature tests, strict all-target Clippy, warnings-denied
rustdoc, and Rust 1.85 all-target checks pass. All-feature tests also pass in the optimized
release profile (83 tests and four doctests). Workspace tests, configured workspace Clippy,
and offline cargo-deny pass with only the previously documented unrelated lint debt. Two
isolated mutations replacing each message-read method with an error are caught by existing
golden-transcript tests. Independent code and documentation review passes after updating
verification status. The fresh two-million-input ASan fuzz run passes with `-max_len=2048`
(457 seconds). Only the five intentional refactor/documentation files were restaged; all
other staged work and the original integration checkout remain untouched. No publication
or hosted CI was performed.

### Explicit server exchange, authorization, and connection (2026-09-16)

The approved public workflow is now implemented for both optional drivers:

```rust,ignore
let server = Server::new(config)?;
let connection = server.exchange(stream)?.authorize()?.connect()?;
// Tokio: server.exchange_async(stream).await?.authorize().await?.connect().await?
let (client, target) = connection.into_parts();
```

This is a scoped API refactor, not the complete version-first folder reorganization.
`Server::new` consumes the existing explicit `ServerConfig` and validates limits before I/O.
Only SOCKS5 CONNECT is implemented. Version-selecting builders, a client facade, and distinct
client/server/proxy folders remain a separate reviewable step; no placeholder versions,
runtime marker traits, dependency, or transport framework are added here.

#### API and policy boundaries

- `Tunnel<S>` is renamed to `Stream<S>` in `src/stream.rs`, including clients and raw-I/O
  handoff. Its buffering, bnb readers, method-local scratch, I/O traits, and extraction
  contracts are unchanged. There is no stale compatibility alias in this unpublished API.
- Driver-level generic `accept` becomes `exchange`; `Incoming<S>` becomes `Request<S>`;
  response methods are `send_success(bound)` and `send_failure(code)`. The wire
  `v5::Request` is unchanged. Generic protected/custom transports remain supported and
  retain caller-managed deadlines and terminal error/cancellation behavior.
- The top-level `Server` uses `exchange` for standard TCP and `exchange_async` for Tokio,
  allowing both features to coexist without overlapping inherent methods. They return
  driver-specific `Exchange` objects containing the authenticated request, peer address,
  and a borrowed immutable configuration. Destination authorization has not yet run.
- `Exchange::authorize` resolves once and filters the numeric candidates against explicit
  policy. All candidates are checked once before dialing, rather than lazily interleaving
  callbacks with connection attempts. Empty resolution remains `InvalidEndpoint`; an
  entirely denied list remains `PermissionDenied`. Policy callbacks remain fast/nonblocking.
- Consuming `Authorized::connect` can dial only the private retained list, without another
  resolution or policy evaluation. Authorization is a snapshot, not continuous policy
  reevaluation. It returns `Connection<S>` owning both the client-side `Stream<S>` and the
  outbound socket, after replying with that socket's actual local bound endpoint.
- `Connection::into_parts` transfers both sides for custom I/O; `relay(timeout)` uses the
  existing half-close-aware relay. No data relay starts during `connect`. The configured
  full proxies compose these stages, preserving listener capacity and shutdown. Blocking
  target registration happens before success so shutdown still owns both relay sockets.
- `Exchange::into_request` preserves the lower-level escape hatch for external policy,
  custom dialing, and manual replies. Blocking handshake socket timeouts are cleared before
  handoff. This escape hatch deliberately cannot prove a caller's authorization or dial;
  its `send_success` contract still requires both to have happened.

#### Deadlines, replies, and ownership

- Blocking DNS remains uncancellable and outside the connection budget. The absolute budget
  begins after DNS and before policy filtering. Tokio starts it before DNS. Both retain the
  same instant across authorization, caller delay, and every connection attempt; a pause
  between `authorize` and `connect` cannot renew it. Callers own time spent holding an
  `Exchange` before beginning authorization or holding an established connection before relay.
- Resolution, authorization, and dial failures attempt an appropriate SOCKS failure reply
  under a fresh bounded reply budget. A reply error takes precedence over the original
  error, matching the previous full-proxy behavior. Once success writing starts, errors
  only close the owned session: no second response may follow a partial success frame.
- Consuming stages prevent managed connect-before-authorization and double connection
  attempts. Dropping a stage closes owned transports without success or automatic dialing;
  async cancellation remains terminal. No stage implements credential-exposing `Debug`.
- The authentication predicate still supplies a boolean, not a principal. Authorization is
  per-peer/original-destination/resolved-address. The new method name does not add identity,
  asynchronous policy backends, rate limiting, or credential zeroization.

RFC 1928 §§3–6 still governs method/request ordering, CONNECT status, and the BND endpoint;
RFC 1929 §2 authentication is unchanged. New stage tests use independently assembled wire
transcripts, retained application payloads, peer-observed bound addresses, and no-dial checks
on ephemeral loopback listeners. Compile-fail examples enforce the consuming API boundaries.
Tokio's paused clock checks deadline retention and a fresh failure-reply budget.

Verification completed before the Mio follow-up: 97 all-feature tests plus nine doctests;
independent default/blocking/Tokio/all-feature tests, strict Clippy, warnings-denied rustdoc,
and Rust 1.85 all-target checks; optimized tests, workspace tests/configured Clippy/MSRV,
and all four offline cargo-deny gates pass. Five automatic policy/deadline mutations and
three manual faults (blocking timeout leakage, renewed Tokio connect budget, and denied-policy
deadline precedence) are caught; one automatic mutation was unbuildable. Two million ASan
session fuzz cases and a final 10,000-case smoke pass. Independent final review cleared the
source/API/security changes after fixing deadline precedence. Documentation bookkeeping was
interrupted by the subsequent docs/Mio requests, not by an outstanding source finding.
This remains local: no commit, push, hosted CI, or release is authorized by this slice. The measured bnb
helper overhead above remains an unresolved delivery decision; this refactor claims no speedup.
Roadmap status remains `dev` because supported versions, commands, and conformance do not change.

### Optional Mio client/server/proxy stack (2026-09-18)

The user requested the complete alternative stack, not merely an adapter example. Feature
`mio` adds cached workspace Mio 1.2.1 (`net`, `os-poll`, no default logging feature), `bnb/net`,
and optional Unix libc for portable `EINPROGRESS` constants at Rust 1.85. No package version
is upgraded and no runtime or Tokio production dependency is enabled. Wire-only defaults,
blocking-only, Tokio-only, and combinations remain supported. The roadmap remains `dev`:
adding another driver does not implement another SOCKS version/command or GSS-API.

#### APIs and ownership

- `mio::Client<S>` and `mio::Exchange<S>` own resumable handshake state on an existing
  nonblocking `Read + Write` transport. Construction does no I/O. `advance`, `interest`,
  and `needs_advance` distinguish readiness waits, fairness continuations, and completion.
  `Exchange` reports `Requested` after authentication and request validation; the embedding
  caller must authorize/dial before `send_success`, or explicitly `send_failure`.
- Partial writes retain both encoded bytes and offset, including a pending flush. Protocol
  phase advancement and subsequent input occur only after the preceding output is flushed.
  Credential checks execute once, even with spurious readiness. `WouldBlock` never becomes
  a session error; every returned error poisons the owned handshake. Borrowed transports
  still require their owner to close them after terminal error.
- The existing bnb 0.6 borrowed reader is reused through a 64-operation budget adapter. It
  retains every accepted byte, enforces the 513-byte cap, retries finite-EOF decoding, and
  batches hints within each call. A budget yield masquerades as `WouldBlock` only internally;
  `needs_advance` tells the scheduler to continue immediately. Decoder attempts/hints are
  restarted after suspension, not a resumable parser within a field. No upstream bnb change
  or additional framing algorithm is needed.
- `take_stream` succeeds only once after full success and preserves the bnb buffer. Raw
  standard I/O on `Stream` now compiles for `blocking || mio`. Direct socket I/O bypasses
  that buffer; it remains limited to registration, configuration, and shutdown.
- `mio::Connector` confirms a numeric TCP connection using `take_error` and `peer_addr`
  after writable readiness. `NotConnected`, `WouldBlock`, and platform in-progress values
  remain pending. It enforces an absolute deadline using its own clock, not a caller-provided
  timestamp that could be stale. It owns no DNS or SOCKS policy.
- `mio::connect_tcp` runs a convenience poll loop under one absolute setup deadline and
  returns **the original Poll**, a deregistered `Stream<mio::net::TcpStream>`, and the bound
  endpoint. Keep/reuse that poll for the stream's lifetime. Mio sources cannot portably be
  moved between polls even after deregistration; the first review caught and removed an
  unsafe promise to drop the private poll at handoff. Native-loop users instead compose
  `Connector` and `Client`, as the runnable client example demonstrates.
- The new `mio/client.rs`, `server.rs`, and `proxy.rs` are distinct role siblings. Existing
  blocking/Tokio phase sequencing is intentionally not rewritten in this feature. All three
  share wire types, strict validators, authentication/destination policy, errors, and lossless
  streams. A common public state engine or the broader version-selecting facade remains a
  separate decision; no dummy future versions or executor abstraction are introduced.

#### Complete proxy and readiness safety

- `mio::Proxy::new(listener, Server)` owns its poll/token namespace. `run` drives it to
  shutdown; `poll(max_wait, report)` allows bounded embedding without an async executor.
  Session errors go to a caller callback. `shutdown_handle` uses an atomic flag and the
  **same** Mio waker used for DNS completions, so blocked polling is interruptible.
- At most `Limits::connections` sessions are accepted. Excess peers stay in the OS backlog.
  The listener is deregistered at capacity and explicitly resumed when a slot is freed.
  Client and target tokens are monotonically allocated and never reused, including failed
  target attempts. Unknown/stale events and DNS results for retired sessions are ignored.
  Token/resource arithmetic is checked; relay buffers are allocated lazily and fallibly.
- Each session drive bounds transport work; the outer loop runs bounded batches and uses
  a deduplicated continuation queue. A yield before `WouldBlock` always schedules another
  turn. Deadlines and shutdown are checked between batches, and timeout calculation includes
  the earliest absolute phase deadline. Ready events and deadline scans do not allocate a
  fresh per-turn vector. Error/closed readiness flags are hints, never substitutes for I/O EOF.
- Two dedicated workers perform system DNS away from the event loop. Request/result channels
  are bounded by connection capacity; submission is nonblocking and saturation fails closed.
  Each answer retains at most 64 numeric candidates. System resolver internals may allocate
  more before exposing the iterator. DNS, all policy checks, and all connection attempts
  share one connect deadline. Authorization filters the retained numeric list once, and
  dialing never re-resolves. Policy elapsed time counts even when every address is denied.
- Successful TCP connect precedes any SOCKS success bytes, using the real local target
  address as BND. Failed resolution/policy/dial queues a resumable failure reply with its
  own budget capped at ten seconds. A failure while writing a success reply only closes
  the session; it can never append a second reply.
- Both 16 KiB circular queues are reserved after TCP connect but **before** queuing success;
  allocation/local-endpoint failures still send a failure reply. Post-success relay assembly
  is infallible. Circular storage never compacts retained payload, including one-byte drains;
  the first review removed a sliding-buffer design with quadratic copying under that load.
- Each established relay owns those two queues. Read interest stops when its direction is
  full; write interest exists only for queued bytes. Draining queues resumes reads in the
  same drive/continuation instead of assuming a new edge. Prefetched client bytes are driven
  immediately. EOF drains queued bytes, then shuts down the opposite write half once while
  preserving the reverse direction. Deadlines terminate both sides.
- Shutdown drops the listener, every active socket, both resolver channels, and queued work.
  It does not join an uninterruptible system resolver. At most two already-running DNS calls
  per proxy may outlive it until the OS call returns; repeatedly constructing proxies around
  a stuck resolver can accumulate such threads. Reuse a proxy instance. Callback execution
  also cannot be preempted: callbacks must remain fast/nonblocking/non-panicking. Their panics
  unwind the Mio loop, unlike the blocking/Tokio workers' `WorkerPanicked` reporting.

#### Evidence and verification

RFC 1928 §§3–6 and RFC 1929 §2 supply the independent transcript vectors. Cached Mio 1.2.1
`Poll` portability documentation and `TcpStream::connect` establish readiness draining,
spurious events, connection confirmation, and lifetime poll association. The Windows SDK
constant confirms WSAEINPROGRESS. No external documentation/package host was contacted.

Verification:

- All-feature suite: 118 tests plus eleven doctests (including three compile-fail examples).
  Default, each independent driver, all three driver pairs, and all-feature tests, strict
  all-target Clippy, warnings-denied rustdoc, and Rust 1.85 all-target checks pass. Both
  runnable Mio examples compile. Optimized all-feature tests also pass.
- Workspace tests, configured all-target Clippy, workspace MSRV, all four offline cargo-deny
  gates, and 45 CI-selector command-contract tests pass. CI's SOCKS profile now includes an
  independent Mio feature suite. Existing unrelated workspace warn-level debt remains;
  historical bnb-macro warnings and the yanked-wnaf advisory remain resolved, not exemptions.
- Mio-only production dependencies exclude Tokio and tokio-util; default builds exclude Mio.
  Cached-target compile checks also pass for aarch64 Linux/musl and big-endian s390x Linux.
  Those are compile checks, not cross-platform execution results.
- RFC transcripts exercise fragmented/coalesced input, partial output and flush, interruptions,
  real `WouldBlock` versus budget continuation, all unknown ATYP values, truncated handshakes,
  maximum credentials, callback-once authentication, downgrade refusal, terminal poisoning,
  and lossless payload handoff. Loopback covers numeric/domain CONNECT, no-dial denial, refused
  target connect, both half-close orders, a 256 KiB pipelined payload, capacity recovery,
  deadlines without readiness, both-socket shutdown, and blocking/Tokio client interoperability.
- Two million ASan cases pass for each of `session` (480 seconds) and the new `mio_session`
  (995 seconds), with maximum input length 2048. Final-source/lock smoke runs cover 10,000
  additional inputs per target. The detached fuzz lock is aligned to production Mio 1.2.1;
  the initial resumable-codec run also exercised the cached compatible Mio 1.2.3 resolution.
  Fuzzing exercises codec/handshake suspension, not kernel readiness scheduling or DNS races.
- Eight selected budget/continuation/half-close mutations are detected. Two initially time out
  in the broad suite and then fail immediate assertions in focused non-network transcripts.
  Five additional circular-queue arithmetic mutations are caught by a bounded tiny-write
  wraparound test. An isolated allocation-failure injection proves that a SOCKS failure reply
  precedes success when relay storage cannot be reserved. No fault was applied to this worktree.
- Independent source/API/security review clears all production findings after the fixes
  recorded above. The final test-harness finding is closed with a bounded progress oracle.
  Source, examples, and rustdoc are available in the existing LAN documentation server.

Linux loopback is the exercised transport; Windows/macOS execution is not claimed. There is
no performance win claim: bounded work/storage and no queue-compaction copies are structural
properties, not a throughput benchmark. The separately measured bnb helper overhead remains an
unresolved delivery choice. All dependency checks used cached/offline data; no commit, push,
hosted CI, PR, tag, or release occurred. The original integration checkout and PNGs are untouched.

Next smallest Mio step: after this review, qualify readiness and socket lifecycle on Windows
and macOS before claiming cross-platform runtime support. A customizable resolver/cancellable
DNS API, asynchronous auth principals, and new SOCKS commands remain separate work.

### Version-first organization and one shared server policy (2026-09-18)

The user approved version-specific client/server implementations under a common setup API,
with an explicit accepted protocol set and ONE server policy across those protocols. This
supersedes the earlier layout deferrals, without adding SOCKS4, another command, or a runtime.

#### Ownership and public API

The initial layout below is refined by the source-ownership cleanup recorded next.

```text
src/
  client.rs              configured client and typed version configuration selection
  server.rs              accepted versions, shared policy/limits, configured server
  server/{blocking,tokio}.rs  exchange -> authorize -> connect stages
  policy.rs              authentication requirements and neutral authorization context
  destination.rs         application IP/domain + port, no wire address discriminant
  limits.rs              shared deadlines and connection capacity
  stream.rs              unchanged lossless bnb input ownership and raw I/O
  io/                    transport writes/deadlines and Mio numeric connector
  proxy/                 blocking/Tokio listeners and complete Mio lifecycle/DNS/relay
  v5/
    wire/                unchanged permissive codecs; also re-exported at v5::
    auth.rs              client authentication choice and server method adaptation
    session.rs           strict V5 validation and reply classification
    io.rs                private bounded Mio codec/write adapter
    client/              V5 config plus blocking.rs, tokio.rs, mio.rs
    server/              blocking.rs, tokio.rs, mio.rs embedded exchanges
```

Root `blocking`, `asynchronous`, `mio`, and `session` modules are removed, not retained as
parallel compatibility facades in this unpublished API. `tokio` now names the backend
accurately; Mio is also nonblocking. Existing wire paths such as `v5::Endpoint` remain valid
deliberately, with their definitions under `v5::wire`. Source movement was first validated
against the existing all-feature suite before changing policy/dispatch behavior.

```rust,ignore
let client = Client::builder()
    .protocol(v5::client::Config::username_password(b"user", b"password")?)
    .build()?;
let policy = Policy::new(ServerAuth::username_password(check_credentials), |context| {
    context.target.ip().is_loopback() // Example only: supply your actual access rules.
});
let server = Server::builder().protocols([Version::V5]).policy(policy).build()?;
let connection = server.exchange(stream)?.authorize()?.connect()?;
```

- Client configuration owns credentials without `Debug`; version-specific generic APIs still
  accept borrowed `ClientAuth`. The facade offers generic blocking/Tokio handshakes and TCP
  convenience methods for all three backends. Domains are forwarded unchanged. Invalid local
  credentials/destinations fail before I/O; a TCP budget spans connect and handshake. Native
  Mio loops use `v5::client::mio::Client`/`v5::server::mio::Exchange` and `io::mio::Connector`:
  this change does not force resumable readiness into a sequential facade contract.
- A typed `client::Protocol` selects the version together with its valid options; no separate
  version/authentication knobs can disagree. There is no public protocol plugin trait or
  fallback retry. `Version`, client protocol, policy operation/authentication, and context
  extension points are non-exhaustive so future versions need not break downstream matches.
- `ServerConfig::new(protocols, policy)` and `Server::builder` require a nonempty version set;
  duplicates are removed. There is no default version set, credential policy, or destination
  permission. All complete proxy entry points validate configuration before accepting work.
- Only `Version::V5` is constructible today. Dispatch uses an exhaustive match on validated
  configuration and delegates the untouched stream to the V5 exchange; it neither sniffs,
  consumes, nor prepends a version byte. Actual multi-version sniff/dispatch is deferred until
  a second codec exists; it must preserve the version byte in the same `BitBuf`. An unsupported
  greeting version now closes without sending a guessed `[5, 0xff]` response. A decoded
  foreign greeting reports `VersionNotAccepted`; truncated input remains a codec/EOF error.

#### Shared security boundary

- `Policy` owns one shared `ServerAuth` and one `Context` predicate. Its clones share the
  callbacks via `Arc`. V5 only adapts the requirement to RFC 1928/1929; it owns no independent
  access policy. Complete blocking, Tokio, and Mio paths all call the same policy evaluator.
- Context carries the peer, a `RequestInfo` (actual validated version, completed authentication,
  operation, original neutral destination), and the exact numeric candidate. Only successful
  exchange constructs that context. The evaluator also rejects an authentication outcome
  inconsistent with its requirement before calling destination policy. No-auth is explicit;
  username/password never falls back. A future protocol unable to satisfy a requirement must
  be rejected, not silently mapped to weaker authentication.
- `Authentication::UsernamePassword` means the shared verifier succeeded; it is not a principal
  or username. Context retains no credential data and has no `Debug`. Claimed identity,
  authenticated principals, asynchronous stores, and per-principal authorization remain a
  separate API decision; a raw claimed identifier must never become verified authentication.
- `Destination` preserves opaque domain bytes and port, or numeric IP and port, with no wire
  discriminant. Conversion alone applies neither protocol validation nor DNS normalization.
  IPv6 scope/flow metadata is not carried by SOCKS endpoints, as before. Version-specific
  clients enforce address-family/length constraints. Managed DNS still requires UTF-8/no-NUL.
- Resolution remains once per request, followed by one callback per retained numeric candidate
  before any dial. Authorization retains an immutable approved list and the SAME absolute
  connect deadline through caller pauses and all attempts. Blocking DNS remains outside its
  connect budget; Tokio/Mio DNS remains inside. Reply-budget precedence, target registration
  before success, partial-success terminality, relay half-close, and Mio continuation/token
  ownership are unchanged. Unsupported commands never reach destination authorization.
- Managed blocking/Tokio exchanges expose neutral destinations/request facts. Their
  `into_request` escape hatch returns the version-specific manual request and keeps its
  caller-owned policy/deadline contract. This is not a second automatic policy path. Adding
  a second accepted version requires an explicit root manual-handoff return/API decision;
  `v5::server::*` remains the direct version-specific manual route. A one-variant wrapper
  is deliberately not introduced ahead of that implementation.
- Neutral request ownership can copy at most 255 domain bytes during a V5 managed exchange;
  Mio also owns a separate resolver query when needed. Owned client credentials are copied
  transiently into each wire exchange. There is no zero-copy, zero-allocation, zeroization,
  or performance-improvement claim. The earlier measured bnb helper overhead remains open.

#### Verification and remaining scope

- 127 all-feature tests and eight doctests (three compile-fail) pass. Independent default,
  blocking, Tokio, Mio, all three backend pairs, and all-feature tests, strict all-target
  Clippy, warnings-denied rustdoc, and Rust 1.85 all-target checks pass. Optimized release
  tests, workspace tests/configured Clippy/MSRV, and all four offline cargo-deny gates pass.
  Unrelated workspace warn-level debt remains; historical bnb-macro warnings and yanked-wnaf
  advisories remain resolved, not exemptions or new failures.
- Configuration tests require explicit protocol/policy choices, reject empty sets and invalid
  bounds/credentials, and canonicalize duplicate versions. Neutral destinations preserve
  IPv4, IPv6, opaque/non-UTF-8/NUL domains, and raw unencodable lengths without applying DNS
  or construction policy during conversion. Configured client transcripts assert exact RFC
  bytes, pre-I/O destination rejection, and coalesced application handoff. Convenience-client
  tests include Tokio and Mio with retained poll ownership.
- Each driver tests every non-V5 greeting byte with no credential callback or guessed reply.
  The complete-backend composition test shares ONE policy across blocking, Tokio, and Mio,
  checks completed authentication/version/operation/original destination/exact numeric target,
  requires one verifier/policy invocation per numeric request, and proves denial prevents dial.
  Existing opaque-domain, malformed credential, no-downgrade, deadlines, cancellation,
  pipelining, half-close, and fairness regressions remain active.
- Six intentional faults in an isolated copy fail immediate assertions: accepting an empty
  version set, ignoring policy denial, reporting the wrong authentication outcome, and
  bypassing each driver's no-reply foreign-version guard. The copy's full suite passes after
  restoration. No fault was applied to the working checkout.
- Both ASan fuzz targets pass two million inputs with `-max_len=2048`: session in 457
  seconds and Mio in 1006 seconds. Final-source smokes pass 10,000 additional inputs per
  target. The detached fuzz workspace also passes strict all-target Clippy. Fuzzing exercises
  codec/session state, not kernel readiness scheduling or DNS races.
- Independent source/API/security and test review finds no remaining issue after marking
  extension points non-exhaustive and documenting manual-handoff evolution.

No dependency, CI selector, release setting, supported version/command, or roadmap stage
changes in this slice. The roadmap stays `dev`; adding organizational boundaries is not
additional protocol conformance. All dependency checks are offline; transport tests use
ephemeral loopback only. No commit, push, hosted CI, PR, tag, or release occurs in this slice.
The original integration checkout and unrelated image assets remain untouched. The existing
bnb helper-overhead delivery decision remains open; this change does not authorize delivery.

Next organization step: when SOCKS4/4A implementation is approved, first build its wire codec,
then its colocated exchanges, and finally add lossless multi-version dispatch and cross-version
policy tests. Do not create empty version modules or copy listener/DNS/relay infrastructure.
The existing `Error` retains precise V5 reply/method/command diagnostics; the next version will
need an explicit error-mapping decision rather than erasing those details into strings.

### Source-ownership cleanup (2026-09-18)

The follow-up review identified root files whose owners were already established. Move
those definitions beside their owners without changing runtime behavior or introducing
another abstraction:

```text
src/
  lib.rs                 exports and the small Version enum
  client.rs              cross-version client configuration
  destination.rs         shared application address model
  error.rs               shared session errors
  server/
    mod.rs               Server, builder, Connection; configuration re-exports
    config.rs            ServerConfig and Limits
    policy.rs            shared authentication and authorization
    blocking.rs          managed blocking stages
    tokio.rs             managed Tokio stages
  io/
    mod.rs               shared transport helpers
    stream.rs            lossless Stream and its I/O implementations
    blocking.rs
    tokio.rs
    mio/
  proxy/                 unchanged complete proxy implementations
  v5/                    unchanged version-first wire/client/server ownership
```

Root `Client`, `Server`, `Connection`, `Stream`, `Destination`, and `Version` remain
available. `server::ServerConfig` also stays unchanged. In this unpublished API,
`socks::policy::*` moves to `socks::server::policy::*`, and `socks::limits::Limits` moves
to `socks::server::Limits`; old root module facades are removed. `config` and `stream`
are implementation modules, not additional public namespaces. Examples, benchmarks,
fuzz targets, tests, and rustdoc imports follow the new paths.

Configuration validation, policy evaluation, buffering, feature gates, and wire bytes
are unchanged. `Version` retains its derives and non-exhaustive contract. No dependency,
roadmap status, protocol support, or publication decision changes. The existing helper
overhead and future multi-version dispatch/manual-handoff decisions remain open. The
next smallest organization step is still a real second version's wire codec, not more
empty modules or generic scaffolding.

Verification for these moves:

- Formatting and all eight feature configurations pass tests, strict all-target Clippy,
  warnings-denied rustdoc, and Rust 1.85 all-target checks. The all-feature suite still
  passes 127 tests and eight doctests; optimized release tests also pass.
- Workspace tests, configured workspace Clippy, workspace MSRV, and all four offline
  cargo-deny gates pass. Unchanged workspace warning-level lint debt remains outside
  SOCKS; the historical bnb-macro warnings and yanked-wnaf advisory are not present.
  The detached fuzz workspace passes strict all-target Clippy.
- Independent review confirms `Policy` and `Stream` moved byte-for-byte, `Version`
  retains its definition, and the extracted configuration and server bodies are unchanged.
  Existing behavior tests are retained rather than adding tests coupled to file placement.
- The session ASan target passes two million inputs with `-max_len=2048` in 461 seconds.
  The fresh Mio run was intentionally stopped at the user's request before two million
  inputs, after its last progress report of 1,451,648 inputs. No finding was reported
  before cancellation; this is NOT a completed qualification pass. The earlier layout
  slice's completed Mio run remains a separate historical result.

### Local verification cadence (2026-09-18)

The user requested lighter routine interactions. Root and SOCKS guidance now distinguish
focused local checks from release qualification. Ordinary code edits retain formatting,
affected behavior tests, strict package Clippy, and relevant review; module moves add only
the useful feature configurations and docs checks. Prose-only edits use diff review and
whitespace checks. Full workspace/feature/MSRV/release-profile suites, fuzz and mutation
campaigns, and benchmarks are reserved for release preparation or an explicit broader-check
request. No CI workflow, selector, hook, scheduled job, or release gate changes in this slice.

### Client facade extraction (2026-09-18)

The user requested a client directory with a facade-only `mod.rs`, including separate small
files for the shared types. This replaces the root `client.rs` shown in the earlier trees:

```text
src/client/
  mod.rs                 documentation, private module declarations, public re-exports
  client.rs              Client state, builder entry point, and version accessor
  builder.rs             Builder defaults, setters, and construction validation
  protocol.rs            Protocol and its version-configuration From implementation
  blocking.rs            blocking impl Client methods
  tokio.rs               Tokio impl Client methods
  mio.rs                 Mio impl Client convenience method
```

No public path or method changes: `socks::Client` and `socks::client::{Client, Builder,
Protocol}` remain available. Backend feature gates move from methods to their private
module declarations. Client state is visible only within the client module tree via
`pub(super)` fields; it is neither public nor crate-wide. Construction checks, credential
ownership, version dispatch, deadlines, transport handoff, and method bodies are unchanged.
There is no new configuration wrapper or protocol abstraction. Server and version-specific
client layouts are outside this extraction, and no roadmap status changes.

Routine-tier verification passes: default SOCKS tests (36 plus one doctest), all-feature
tests (127 plus eight doctests), formatting, strict all-target/all-feature SOCKS Clippy,
and warnings-denied rustdoc. Independent review confirms unchanged behavior, feature gates,
privacy, and public paths, with no findings. No workspace, exhaustive matrix, benchmark,
mutation, or fuzz campaign was started for this structural change.

### Facade-only source cleanup (2026-09-18)

The user approved the remaining organization review recommendations and checking for empty
source paths. `lib.rs` and all `mod.rs` files now contain only documentation/attributes,
alphabetically grouped module declarations, and re-exports. Definitions stay beside their
implementations in purpose-named files; small files do not justify placing behavior in a
facade. This supersedes the earlier root-inline `Version` decision and layout trees.

```text
src/
  lib.rs                 crate facade
  version.rs             Version, re-exported at the crate root
  client/                existing client split; declarations precede re-exports
  server/
    mod.rs               server facade
    server.rs            Server and shared setup methods
    builder.rs           Builder
    connection.rs        Connection and lossless handoff
    config.rs            unchanged ServerConfig and Limits
    policy.rs            unchanged shared policy
    blocking.rs
    tokio.rs
  io/
    mod.rs               transport facade
    address.rs           existing domain-name conversion/validation
    deadline.rs          shared timeout error construction
    mio/
      mod.rs             connector facade
      connector.rs       unchanged numeric connection mechanics
      deadline.rs        existing explicit-instant deadline arithmetic
  proxy/mio/
    mod.rs               Proxy/Shutdown facade
    proxy.rs             poll loop, listener, token registration, and scheduling
    entry.rs             existing per-connection Entry and State
    shutdown.rs          shutdown handle
    relay.rs             unchanged bounded relay
    resolver.rs          unchanged resolver workers
  v5/
    mio_io.rs            renamed private Mio codec adapter (formerly io.rs)
    client/config.rs     Config extracted from the client facade
    wire/version.rs      VERSION extracted from the wire facade
```

All public paths, feature gates, wire constants, signatures, and method bodies are preserved.
The server builder/configuration/policy, V5 credentials, deadline budgets, authorization,
numeric target pinning, partial-write offsets, token lifecycle, and shutdown behavior do not
change. Mio's `Entry`, `State`, and shared fields/methods use only `pub(super)` where sibling
files need access; none becomes crate-wide or public. The existing crate-visible server and
I/O internals retain their visibility. No wrapper types, dependencies, commands, versions,
roadmap changes, or publication are introduced.

The source scan found no empty files and one obsolete empty directory, `src/mio/`, left by
the earlier version-first moves. That directory was removed with non-recursive `rmdir`;
it contained no data or tracked files. No unrelated files or directories were removed.

Routine verification passes default SOCKS tests (36 plus one doctest), all-feature tests
(127 plus eight doctests), formatting, strict all-target/all-feature SOCKS Clippy, and
warnings-denied rustdoc. The smaller extraction was also compiled before the Mio split.
Independent review confirms behavior, public paths, feature gates, and scoped visibility
are preserved, with no findings. The final source scan confirms all eleven facades contain
only documentation/attributes, ordered declarations, and re-exports, with no remaining
empty source files or directories. No workspace suite, exhaustive matrix, benchmark,
mutation campaign, or fuzz run was started; release qualification remains separate.

### Shared vocabulary grouping (2026-09-18)

The user approved grouping `Destination` and `Version` beneath private `src/types/`, with
a facade-only `mod.rs` and unchanged definitions in `destination.rs` and `version.rs`.
This supersedes their root file locations in the earlier layout trees. Public
`socks::Destination` and `socks::Version` remain available with default features; the folder
is not a new public namespace or a home for miscellaneous helpers. Conversion impls remain
beside their target types. `src/error.rs` and `socks::error::Error` stay unchanged because
the guided API's failure boundary spans client, server, proxy, and I/O.

No behavior, feature, wire format, dependency, or roadmap status changes. The current shared
error retains V5-specific method, command, and reply values. Its separation from future V4
failures remains a design decision for that implementation, not part of this source move.
The next protocol and platform qualification steps below remain unchanged.

Routine verification passes formatting, default SOCKS tests (36 plus one doctest),
all-feature tests (127 plus eight doctests), strict all-target/all-feature SOCKS Clippy,
warnings-denied rustdoc, and whitespace checks. Independent review confirms byte-identical
type definitions, stable public paths and feature availability, and matching documentation.
No tests coupled to file placement, broader workspace checks, or release qualification
were added; existing behavior tests cover the move.

### Backend boundaries and navigation refinements (2026-09-18)

The approved structural review is implemented without adding V4 placeholders or runtime
abstractions. Blocking/Tokio complete proxy entry points now borrow a validated `Server`,
matching the builder-driven Mio path while retaining backend-appropriate ownership. This
is an intentional signature change in the unpublished API: replace `&config` with a borrowed
`Server::new(config)?` or a server built with `Server::builder()`. Listener workers clone
the validated server and its configuration, sharing the same policy callbacks, without
reconstructing or revalidating the server per connection. Configuration validation is now
private to the server module.

Mio `Entry` constructs its version-specific exchange and accepts resolution results only
while resolving. Protocol state, request facts, peer, identity, and pending resolution are
private to the entry implementation. Poll scheduling and registration metadata remain
available to `Proxy`; stale answers still cannot revive another phase. Numeric address
conversion reuses `Destination::socket_addr`. Blocking/Mio deadline adapters share arithmetic
with explicit `now`, preserving zero/overflow rejection, exact expiry, error classification,
and absolute budgets. Tokio continues to use Tokio time for its timeout behavior.

The blocking proxy now separates listener/session management from relay mechanics, the V5
Mio client separates its resumable handshake from its convenience poll loop, and `Stream`
separates buffer ownership from standard/Tokio I/O implementations. Their public paths and
feature availability are unchanged; all new `mod.rs` files are facades. The current map above
supersedes historical trees without deleting their rationale or verification evidence.

Existing listener tests now exercise builder-created blocking/Tokio servers; the three-backend
policy regression shares clones of one builder-created server, retaining its callback-count,
verified-context, and no-dial-on-denial assertions. Other callers explicitly validate their
configuration before invoking complete proxies. No new file-placement tests or test harness
were introduced. No dependency, wire format, roadmap stage, or publication changes occur.
V4 dispatch, manual-handoff types, version-specific errors, and receive bounds remain deferred
until its wire codec is implemented; the next protocol/platform steps below are unchanged.

Routine verification passes default tests (36 plus one doctest), all-feature tests (127 plus
eight doctests), formatting, strict all-target SOCKS Clippy with all features and separately
with Mio/Tokio, and warnings-denied all-feature rustdoc. Targeted backend compilation also
passes. The initial Mio-only compile exposed a newly unused timeout re-export after the
arithmetic extraction; gating that re-export to Tokio resolved it, and the final strict
checks are warning-free. The source scan confirms all fifteen facades remain declaration-
and-export-only, no empty source files/directories remain, and the current ownership map
matches the source tree. Independent review found no source/API/security or test issue;
its configuration-cloning documentation correction is resolved. No workspace suite,
exhaustive feature/MSRV matrix, benchmark, fuzz, mutation, or release gate was run.

### Message-owned guided checks (2026-09-18)

The former `v5/session.rs` contained neither a session type nor lifecycle state. Its
message-specific behavior now lives in inherent implementations beside the corresponding
wire types: `Endpoint::check_length`, `MethodSelection::check_offered`,
`UsernamePasswordResponse::ensure_success`, `Request::check_header`, and
`Reply::ensure_success`. These are explicit crate-private guided checks, enabled only with
a driver feature, not decode hooks or replacements for bnb's construction `validate` methods.
A failing reply and a BIND request remain representable wire messages, not invalid types.

`Reply::success` and `Reply::failure` own compliant guided reply construction. Success checks
the bound endpoint; failure uses the existing unspecified IPv4 endpoint and rejects both
the named success code and its raw `Other(0)` alias. The raw builder, fields, decoding, and
canonical/verbatim encoding remain unchanged. Constructors do not authorize or dial targets;
those responsibilities remain with the existing server stages and embedded callers.

`v5/server/validation.rs` composes request-header checks, CONNECT-only capability enforcement,
and destination checks in their original order. `v5/wire/validation.rs` retains only shared
version/reserved-byte checks. `v5/decode.rs` retains the unchanged failed-command header probe
and cursor restoration, because no decoded message exists on that path. `session.rs` is
removed; no wrapper type or public API is added. All three backends use the same methods.

Targeted transcript regressions cover header/capability/destination error precedence,
authentication-version precedence without sending CONNECT, and failure-reply precedence over
an invalid bound address. They also prove raw decoding and verbatim encoding still preserve
the rejected wire values. Existing authentication, reply-byte, unknown-ATYP, and lossless
handoff regressions remain the behavioral oracle for the relocations.

Routine verification passes the three targeted regressions, default tests (36 plus one
doctest), all-feature tests (130 plus eight doctests), formatting, strict all-target/all-feature
SOCKS Clippy, and warnings-denied rustdoc. Clippy initially flagged borrowing the tiny `Copy`
`MethodSelection`; its method now takes `self`, matching the old helper's by-value semantics,
and final checks are warning-free. Independent review reports no findings. No workspace,
exhaustive feature/MSRV matrix, benchmark, fuzz, mutation, or release gate was run.

No dependency, runtime behavior, supported command/version, roadmap status, or publication
changes occur. The existing performance and V4 dispatch/error/handoff decisions remain
open. The next protocol and platform qualification steps below are unchanged.

### Domain-owned construction and validation (2026-09-18)

The user approved extracting the domain variant into a payload type rather than making
an endpoint-wide length helper inspect unrelated IP variants. `v5::Domain` (also available
under `v5::wire`) owns the length-prefixed name and network-order port; `Endpoint` still
supplies the single `ATYP = 3` byte. `Domain` uses bnb's existing struct codec/builder with
`validate = Domain::check_length`. No upstream bnb change, dependency, runtime, allocation
wrapper, or generic validator is introduced. IPv4/IPv6 variant shapes remain unchanged.

`Domain::check_length` is the one length rule, with public `thiserror` diagnostics
`DomainError::Empty` and `DomainError::TooLong { length }`. It and the generated
`Domain::{validate,is_valid}` are available without driver features. `Endpoint::validate`
only dispatches to that domain check; it replaces the former crate-private
`Endpoint::check_length`. Generated request/reply builders now explicitly validate their
nested endpoints, including raw or mutated domain payloads. They do not reject BIND,
UDP ASSOCIATE, failure replies, or noncanonical header fields: capabilities, outcomes,
and representation remain separate. Builder errors retain bnb's usual `Invalid` boundary.

This deliberately changes the unpublished enum API from `Endpoint::Domain { name, port }`
to `Endpoint::Domain(Domain { name, port })`. For checked construction, use
`Domain::builder().name(name).port(port).build()?` and convert via `Endpoint::from`/`into`.
`Domain`'s fields remain public; `Endpoint::domain(name, port)`, `From<Domain>`, and neutral
destination conversions preserve raw data without validating or normalizing it. A checked
builder is not a permanent validity guarantee after mutation. Standalone `Domain` encoding
contains only length/name/port, not the address-type byte or request/reply headers.

Guided clients and servers recheck current values at the same boundaries, mapping domain
errors to the existing `Error::InvalidEndpoint` contract. Version/reserved/command/status
error precedence and failure reply codes are unchanged. DNS resolution still owns UTF-8
and NUL restrictions; length validation accepts opaque/non-UTF-8 bytes and performs no
resolution, IDNA conversion, or label checks. Empty-domain messages remain decodable and
writable verbatim; canonical request/reply encoding repairs header constants, not names.
Oversized names remain unencodable. Existing session fuzz targets reach this payload via
request/reply decoding; no separate framing implementation or fuzz campaign is added.

New contract/adversarial tests cover exact payload and enclosing bytes, 1/255-byte bounds,
empty/256-byte construction failures, nested-builder enforcement, typed errors, post-build
mutation, every truncated payload prefix, opaque bytes, and permissive-versus-canonical
behavior. Existing transcript and lossless-handoff suites cover all three driver integrations.
The scalar `wire/validation.rs` helper cleanup remains separate from this domain refactor.

Routine verification passes focused contract/adversarial tests (36), default tests (42 plus
two doctests), all-feature tests (136 plus nine doctests), formatting, strict all-target
SOCKS Clippy with default/all features, and warnings-denied all-feature rustdoc. The first
test draft assumed standalone domain/endpoint canonical helpers that bnb does not generate;
canonical behavior is instead verified through request/reply messages with reserved fields.
Final checks are warning-free; independent review reports no findings in codec layout,
validation, API boundaries, feature isolation, or test coverage. No workspace suite,
exhaustive feature/MSRV matrix, benchmark, fuzz, mutation, or release gate was run.

No supported version/command, roadmap status, release setting, or publication changes.
Existing bnb performance, V4 dispatch/error/handoff, and platform-qualification decisions
remain open; the next smallest protocol/platform steps below are unchanged.

### Checked endpoints and CONNECT destination ports (2026-09-19)

`Endpoint::domain(name, port)` now returns `Result<Endpoint, DomainError>` and calls
`Domain::check_length`; `Endpoint::domain_raw` retains the old unchecked constructor.
This intentionally changes the unpublished convenience API. Checked construction still
accepts byte vectors: it requires 1–255 bytes, not UTF-8, DNS label syntax, NUL exclusion,
IDNA conversion, or successful resolution. Raw fields and conversions remain available;
empty domains still round-trip and oversized names remain unencodable. Canonical encoding
does not repair names or ports. No new dependency, normalization, or allocation is added.

`Endpoint::validate_connect_destination` composes domain-length validation with a nonzero
destination-port check. All blocking/Tokio/Mio clients call it before handshake I/O; TCP
convenience entry points call it before dialing the proxy. All server backends check the
request header, supported command, domain length, then destination port, before exposing a
request to manual handlers or managed resolution/authorization/dialing. The typed
`Error::ZeroDestinationPort` maps through the existing general-failure reply fallback.
That mapping is an implementation choice, not a prescribed RFC 1928 zero-port response.
It must not masquerade as an attempted connection's `ConnectionRefused` outcome.

Port zero remains valid in the general endpoint/domain and request/reply construction
APIs and codecs. In particular, failure replies keep their unspecified `0.0.0.0:0`
bound address. Future BIND/UDP address rules must not reuse the CONNECT-specific check.

Regression coverage includes checked versus raw domain construction, opaque bytes,
empty/1/255/256-byte lengths, raw/canonical zero-port wire messages, all three address
families with ports 0/1/65535, and each driver's rejection before client reads/writes or
proxy dialing and before server request handoff. Exact server failure transcripts retain
zero bound ports; a client transcript accepts a successful zero-port bound endpoint and
retains its coalesced application payload. Compound malformed requests preserve
header/command/domain precedence.
Existing refusal tests now reserve a nonzero, bound-but-not-listening TCP socket instead
of using port zero, retaining deterministic coverage of actual destination dial failures.

Routine verification passes default tests (44 plus three doctests), all-feature tests
(143 plus ten doctests), formatting, strict all-target SOCKS Clippy with default/all
features, and warnings-denied all-feature rustdoc. Cargo ran offline; transport tests used
ephemeral loopback sockets only. Independent review found no source/API defects; its
requested successful-zero-bound-port regression is included, with no remaining findings.
No workspace suite, exhaustive feature/MSRV matrix,
benchmark, fuzz, mutation, or release gate was run.

No supported command/version or roadmap status changes. DNS/IDNA policy and future
version-specific error boundaries remain separate decisions; the next smallest protocol
and platform steps below are unchanged.

### Typed dispatch-error conversion (unreleased candidate, 2026-09-19)

The local bnb runtime/macro candidate supplies type-attributed closed-enum dispatch
diagnostics. `From<bnb::BitError> for Error` recognizes only `Endpoint` misses carrying an
integer that fits `u8`, returning `UnsupportedAddressType(code)`. Wrapped reader codec
errors enter the same conversion. Other origins, missing/noninteger observations,
oversized integers, payload truncations, and unrelated errors retain the original codec
error, position, and field. Enum names and formatted text are never classification keys.

The command-header probe and its cursor restoration are removed, along with the extra Mio
command-read wrapper. Blocking/Tokio exchanges use their ordinary message reads; Mio uses
ordinary `receive`. The wire `Request`/`Reply` APIs continue returning `bnb::BitError`;
only the guided boundary translates it. Unsupported ATYP still has no known payload width
and still maps to the existing address-type-not-supported reply. No header type, policy,
wire format, framing rule, or transport state is added.

Tests cover direct/wrapped conversion, wrong originating types with the same diagnostic
name, defensive observation handling, all unsupported address bytes, fragmented/coalesced
driver transcripts, and unchanged truncation classification. The current ownership map
and AGENTS guide no longer assign failed-decode classification to a separate probe module.

This uses the typed dispatch error released in bitsandbytes 0.7.0 (2026-09-21); the
workspace requires the macro crate at exactly the same version. bnb's buffered success path
executes the same instruction count per message as before the feature, and a terminal
dispatch miss costs 7–13 ns more, which ends the session anyway. This is not a SOCKS session
benchmark; removing the probe alone is not evidence of a net speedup. Measurements are in
bnb DESIGN §13. SOCKS itself remains unmerged work on its own branch, not published or
released. No protocol roadmap status or release setting changes. The next protocol slice
remains the blocking BIND exchange described below.

## Known limitations

- Unknown `ATYP` cannot be decoded as a typed `Endpoint`: RFC 1928 assigns no payload width, so a
  stream parser cannot locate the port. The address code itself remains representable as
  `AddressType::Other`; arbitrary whole frames belong in the next raw-codec slice.
- Only the guided CONNECT subset above has authentication, transport, and proxy behavior.
- RFC 1929 credentials are plaintext byte vectors. The wire codec does not provide secrecy or
  zeroization and deliberately does not expose the credential-bearing request through `Debug`.
- The crate is not yet re-exported by the `rsl` facade.
- Fuzzing covers hostile client/server input and lossless handoff through the blocking and
  resumable Mio handshakes. Actual Mio poll/socket scheduling and Tokio cancellation are
  exercised by integration tests, not libFuzzer.

## Protocol dependency map

1. Preserve the SOCKS5 wire/session seam now shared by blocking, Tokio, and Mio CONNECT. Extend the
   raw surface independently; later BIND and UDP ASSOCIATE must not couple wire types to sockets.
2. Reuse the existing TCP/UDP and DNS concepts at behavioral boundaries, but use ordinary stream
   and datagram transports for proxying; raw packet injection is not a SOCKS prerequisite.
3. Use bnb's reusable incremental core, verified upstream with multiple wire shapes and adopted
   here for SOCKS. Other stream protocols own their negotiation/policy and may use bnb adapters
   or their own I/O; do not generalize SOCKS session state into their framing contracts.
4. TLS should consume PKI validation and `rsl-crypto` primitives; SSH should consume crypto
   primitives but owns its packet, negotiation, transcript, and host-key rules. Neither is a
   prerequisite for SOCKS, though SOCKS CONNECT can later carry either transparently.
5. SMB depends on TCP plus its own framing and commonly composes with NBT, DCERPC, authentication,
   and directory protocols. DCERPC should precede broad SMB management APIs. LDAP and Kerberos
   need ASN.1/DER and crypto selectively, but protocol transcripts and policy stay out of PKI and
   crypto. These are later application-layer consumers, not reasons to generalize SOCKS types.

## Next smallest reviewable step

For Mio, qualify the existing readiness and socket lifecycle on Windows/macOS before claiming
cross-platform runtime support. No additional protocol command is needed for that check.

The version-first organization and explicit configuration/shared-policy slice above is now
implemented. A future SOCKS4/4A slice must start with its wire codec, then prove cross-version
dispatch and policy consistency; no placeholder or claim of multi-version runtime support
is included here. The bnb optimization decision remains independent.

The next protocol extension remains a blocking BIND embedded-session slice: two distinct
replies and their deadlines, with
explicit inbound-peer authorization. Prove that handshake before adding listening-proxy and
Tokio BIND support. UDP ASSOCIATE remains separate because it needs datagram framing and
source pinning. The raw/malformed convenience surface remains independently deferred.
