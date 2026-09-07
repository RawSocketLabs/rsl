# SOCKS integration note

**Status:** SOCKS5 wire codecs plus blocking/Tokio CONNECT client and server slices implemented.
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
messages on `bnb`. Optional blocking and Tokio session layers now compose these codecs into
CONNECT clients, embedded server handshakes, per-connection proxies, and bounded listeners.

The original audit used `refactor/netlink-protocol-location`, three commits ahead of and four
behind `origin/main` at that point. Its unrelated untracked image assets were left untouched.
The current delivery candidate is main-based; see the delivery section below.

## Decisions

- Use `bnb` for the wire codec and the workspace's `thiserror` for semantic construction
  diagnostics. The default wire surface needs no transport or runtime dependency; optional
  `blocking` and `tokio` features add the reviewed CONNECT behavior described below.
- Name wire messages by their role (`MethodRequest`, `MethodSelection`, `ReplyCode`) instead of
  preserving the draft's ambiguous `Identifier`, `Offer`, and `Response` API.
- Make `Endpoint` own `ATYP + address + port`. The draft stored `address_type`, address, and port
  separately, allowing ordinary typed construction to disagree.
- Preserve domain bytes rather than require UTF-8. The one-octet length is derived and checked.
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

## Verification

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

## Known limitations

- Unknown `ATYP` cannot be decoded as a typed `Endpoint`: RFC 1928 assigns no payload width, so a
  stream parser cannot locate the port. The address code itself remains representable as
  `AddressType::Other`; arbitrary whole frames belong in the next raw-codec slice.
- Only the guided CONNECT subset above has authentication, transport, and proxy behavior.
- RFC 1929 credentials are plaintext byte vectors. The wire codec does not provide secrecy or
  zeroization and deliberately does not expose the credential-bearing request through `Debug`.
- The crate is not yet re-exported by the `rsl` facade.
- Fuzzing covers hostile client/server input through the shared framer and blocking driver;
  Tokio-specific scheduling/cancellation is exercised by integration tests, not libFuzzer.

## Protocol dependency map

1. Preserve the SOCKS5 wire/session seam now shared by blocking and Tokio CONNECT. Extend the
   raw surface independently; later BIND and UDP ASSOCIATE must not couple wire types to sockets.
2. Reuse the existing TCP/UDP and DNS concepts at behavioral boundaries, but use ordinary stream
   and datagram transports for proxying; raw packet injection is not a SOCKS prerequisite.
3. Build a narrow reusable stream-framing/transport contract only when both SOCKS and a second
   stream protocol (likely SSH or TLS) demonstrate the same need.
4. TLS should consume PKI validation and `rsl-crypto` primitives; SSH should consume crypto
   primitives but owns its packet, negotiation, transcript, and host-key rules. Neither is a
   prerequisite for SOCKS, though SOCKS CONNECT can later carry either transparently.
5. SMB depends on TCP plus its own framing and commonly composes with NBT, DCERPC, authentication,
   and directory protocols. DCERPC should precede broad SMB management APIs. LDAP and Kerberos
   need ASN.1/DER and crypto selectively, but protocol transcripts and policy stay out of PKI and
   crypto. These are later application-layer consumers, not reasons to generalize SOCKS types.

## Next smallest reviewable step

Add a blocking BIND embedded-session slice: two distinct replies and their deadlines, with
explicit inbound-peer authorization. Prove that handshake before adding listening-proxy and
Tokio BIND support. UDP ASSOCIATE remains separate because it needs datagram framing and
source pinning. The raw/malformed convenience surface remains independently deferred.
