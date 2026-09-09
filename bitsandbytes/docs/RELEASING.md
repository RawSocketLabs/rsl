# Releasing

Versions are **derived from [Conventional Commits](https://www.conventionalcommits.org)**
by [release-plz](https://release-plz.dev). You never hand-edit a version number.

## How it works

1. **Every commit must be conventional.** `.github/workflows/commitlint.yml` lints
   every commit in a PR against `commitlint.config.mjs`. A non-conforming message
   fails CI. The squash title must also carry the intended scope and breaking marker:
   release-plz parses the history that actually reaches `main`.

2. **Successful CI unlocks release automation.** After push CI succeeds for the
   exact current `main` SHA,
   `.github/workflows/release-plz.yml` opens (or updates) a **release PR** that, for
   each enabled crate, bumps its version from the commits since its last tag and updates its
   `CHANGELOG.md`. The bump rule (pre-1.0, i.e. `0.x`):

   | commit                                   | bump          |
   | ---------------------------------------- | ------------- |
   | `fix:`                                   | patch         |
   | `feat:`                                  | patch (`0.x` default) |
   | `feat!:` / `fix!:` / `BREAKING CHANGE:`  | minor (`0.x`) |
   | `docs:`                                  | release-eligible; inspect generated PR |
   | non-breaking `chore`/`ci`/`refactor`/`test`/… | no bump    |

   This uses release-plz's default `features_always_increment_minor = false`:
   an additive feature on `0.4.0` selects `0.4.1`, not `0.5.0`. Inspect the
   generated candidate; version numbers remain automation-owned.

   The two crates use **independent versions**; if `bitsandbytes-macros` bumps,
   release-plz also bumps `bitsandbytes` (it depends on it) and rewrites the
   `version = "…"` pin in the root `Cargo.toml`.

3. **Merging the release PR cuts the release.** The `release-plz release` job then
   creates the git tag(s) — name-prefixed per crate (`bitsandbytes-v0.3.1`,
   `bitsandbytes-macros-v0.3.1`, …) — and publishes the crates that opt in to
   crates.io (see below).

**Release processing and crates.io publishing are independently opt-in.** The
workspace defaults in `release-plz.toml` are `release = false` and `publish = false`.
Only `bitsandbytes` and `bitsandbytes-macros` enable both. Netlink retains its
`publish = true` setting but inherits `release = false`, so it is not versioned,
tagged, or published. Other pending releases are held, not discarded from history.
GitHub Releases remain disabled (`git_release_enable = false`).
(Versions up to 0.3.1 were published by hand before this automation existed.)

## CI and release ordering

The `workflow_run` gate verifies the CI workflow path, successful push event,
repository, branch, and SHA using GitHub's API. Superseded runs do nothing. Each
write-capable job checks its checkout against that SHA before release-plz runs.
Workflow-level concurrency serializes release and PR updates; the PR job waits for
tagging/publishing so it cannot regenerate a release against stale tags. A new push
after verification can supersede the candidate; the workflow does not lock `main`.

Dependency, policy, and workflow changes trigger bnb's full CI gates, including
strict all-target/all-feature Clippy, warning-free rustdocs, feature tests, MSRV,
bare-metal renamed-dependency compilation, public API, source compatibility,
fuzzing, and cargo-deny. `actionlint` checks every workflow. The compatibility
baseline is explicitly published `0.5.0`; advance it deliberately after releases.
Compatible additions enforce `--release-type patch` in all three feature modes.
The breaking 0.4 → 0.5 transition used major mode plus exact reviewed API deltas.
The three `bnb/api-delta-0.5-{all,default,none}.txt` snapshots remain historical migration
evidence, not executable CI gates. The one-time delta checker was removed after publication;
the standing public-API snapshot check and all three patch compatibility gates remain.
The checker cannot certify proc-macro expansion or behavior: consumer/UI tests
and review remain required.

Do not recreate historical tags. The `git_only` migration remains deferred in
`release-plz.toml` until a packageable tag baseline exists.

## Pre-commit correctness and performance gate

CI success is necessary, not sufficient. Before committing a runtime or macro release:

1. Inventory runtime, macro expansion, public contracts, and their existing evidence.
   Review shared dependencies of changed paths as well as the diff. Record findings in
   `bnb/DESIGN.md`: location, consequence, evidence, severity, pre-existing/new status,
   disposition, and the regression check that closes each finding. Independent unrelated
   improvements belong in `bnb/ROADMAP.md`; pre-existing release blockers are not waived.
2. Pin an immutable baseline revision and representative consumers. Record toolchain,
   features, corpus, CPU/allocator environment, and benchmark settings. Establish baseline
   variance and a justified regression budget before measuring the candidate. Keep allocation
   instrumentation separate from uninstrumented throughput measurements. Measure scaling,
   copying, retention/allocation, I/O calls, and representative codegen/build growth;
   bounded memory alone does not establish bounded CPU work.
3. Prove changed contracts with specification vectors, independent/reference results, and
   stateful properties. Incremental decoding needs arbitrary partitions and sequences of
   feed/decode/EOF/compaction/rejection/handoff, not just slice fuzzing. Account for consumed
   input and retained tails. Exercise real non-SOCKS consumers, custom/context codecs, both
   bit orders, and byte-padded versus tightly packed messages. Distinguish buffering bounds
   from allocations owned by decoded values.
4. Fuzz the actual changed paths and test the tests through focused mutation. Surviving
   non-equivalent mutations in consumption, capacity, EOF, dispatch, or extraction must be
   resolved; record equivalent mutations and timeouts separately. Use deterministic resource
   and progress assertions in CI, not timing thresholds on shared runners. Controlled local
   benchmarks remain the performance approval gate.
5. Run the required formatting, strict Clippy, feature/workspace tests, denied-warning docs,
   MSRV, bare-metal no_std, macro/UI and renamed-consumer checks, public API/compatibility,
   fuzz, and package checks. Relevant workflow/dependency changes also require actionlint and
   cargo-deny. Record exact commands, results, limitations, and baseline-only failures.
6. Obtain independent reviewer approval of the final post-fix diff, findings ledger, test
   evidence, and measurements. Resolve release blockers and unexplained material regressions;
   document accepted nonblocking tradeoffs and follow-ups. Later edits invalidate affected
   checks/review. Stage only intentional files and commit only after this gate is satisfied.

This process does not authorize a generic parser rewrite, unsafe optimization, unrelated
cleanup, or release side effects. A measured design limitation that defeats an intended use
must be resolved or the proposed adoption revised before claiming that use is ready.

### Coordinated incremental 0.5.0 release

Both runtime and macros shipped as **0.5.0**: removed/changed runtime APIs are breaking, and
new macro expansion calls new runtime helpers. Publishing the new macros under `^0.4`
would have permitted Cargo to pair them with an incompatible old runtime. Release-plz
generated both versions and the matching dependency requirement in
[PR #77](https://github.com/RawSocketLabs/rsl/pull/77). Both archives were verified before
publication; [release automation](https://github.com/RawSocketLabs/rsl/actions/runs/34298309943)
published them from `91f87b7b`. Both registry checksums and crate-prefixed tags were verified.
See the delivery receipt in `bnb/DESIGN.md` §11.7.

For future releases, repeat archive verification on the generated release PR's exact
contents. Also regenerate the detached `bitsandbytes/fuzz/Cargo.lock` for new path
package versions (`cargo metadata --manifest-path bitsandbytes/fuzz/Cargo.toml --format-version 1`).
CI checks this lockfile with `--locked` before cargo-fuzz; release-plz does not own detached
workspace lockfiles. Do not merge the version PR with a stale fuzz lock.

## Required: a token that can open the release PR

A token that acts as a **user or GitHub App** is required: it makes CI run on the
release PR and avoids the previously observed org restriction on Actions-created
PRs. Set `RELEASE_PLZ_TOKEN`; there is deliberately **no `GITHUB_TOKEN` fallback**.
Use one of:

- **A fine-grained PAT** (simplest) — repository access to this repo, with
  **Contents: read/write** and **Pull requests: read/write**.
- **A GitHub App token** (no human owner, auto-rotated) — install an app with the same
  permissions and mint the token via `actions/create-github-app-token` in the workflow.

Both `RELEASE_PLZ_TOKEN` and `CARGO_REGISTRY_TOKEN` must be nonempty before the
release job starts any tag/publish side effects. Presence is not proof of scope,
ownership, or validity; an administrator must verify those separately. Never print
token values. The workflow uses the ordinary `GITHUB_TOKEN` only for release
operations that do not need to trigger a PR's CI.

## Publishing to crates.io

The `release-plz-release` job reads the `CARGO_REGISTRY_TOKEN` repository secret — a
crates.io token with publish scope for `bitsandbytes` and `bitsandbytes-macros`. The
two crates are published in dependency order: the macro version must exist on the
registry before the runtime is published. Locally, verify both unpublished archives
together with `cargo package -p bitsandbytes-macros -p bitsandbytes --all-features
--allow-dirty`; Cargo's temporary registry supplies the matching macro archive.
This builds the actual archive contents without uploading them; inspect the package
file lists as well. `--allow-dirty` is for candidate verification, not publication.
If the
secret is missing, preflight fails before release-plz runs. Publication across
crates is not transactional; after a registry or credential failure inspect the
actual tags and registry versions before deciding how to retry.

To release **another** workspace crate, review its pending changes, version,
changelog, and ownership, then explicitly enable `release = true` in its
`[[package]]` entry. Also enable `publish = true` if registry publication is intended.
Never flip either workspace default. Several
workspace crate names (`ethernet`, `arp`, `udp`, `ip`, `dns`, …) already exist on
crates.io as unrelated projects, so a blanket `publish = true` would attempt uploads
to names we do not own. Check ownership of the crates.io name first
(`cargo owner --list <name>`).

## Additive runtime release after 0.4.0

Both `0.4.0` crates were published successfully under `michael-smythe` after
PR #65 merged as `a8d1bc5`; hosted CI and release automation passed. This confirms
the registry token's publication path, superseding the historical token blocker
below. Keep the release allowlist restricted to the bnb pair.

The consuming enum-alias helper adds only a default runtime trait method. Expect
`bitsandbytes 0.4.1` with `bitsandbytes-macros 0.4.0`; review the generated versions,
changelog, dependency pin, and archive before merging its release PR. Verify the
runtime archive against the already-published macro crate with `cargo package
-p bitsandbytes --all-features --allow-dirty`. No macro release is warranted by
this feature, and no manifest or changelog is hand-bumped for delivery.

## Historical 0.4.0 preparation

Builder enum-alias normalization changes observable construction behavior, even
though the public trait is additive and decoding/verbatim encoding retain their
existing behavior. Preserve a breaking Conventional Commit marker for the bnb
change (for example `feat(bitsandbytes)!: normalize builder enum aliases`) and
inspect the generated release PR for the intended `0.4.0` runtime and compatible
macro dependency. Do not hand-bump manifests or regenerate release changelogs.

The production MSRV remains Rust 1.85, including all runtime features and the
renamed no_std consumer. Developer tests/benchmarks use stable Rust; current
trybuild and Criterion require newer compilers and do not raise the library MSRV.

Dependency preparation refreshes syn to 3.0.5, updates benchmark-only bitbybit to
2.0.1 (removing its old arbitrary-int 1.x dependency), and replaces yanked wnaf
0.14.0 with 0.14.1. Other bnb direct dependencies were checked against registry
metadata; no additional direct update was indicated. Unrelated facade/blessed-stack
updates are outside this release's scope.

Local checks are necessary, not approval to publish. Require successful hosted CI
for the delivered candidate, verified token scopes/ownership, review of generated
versions/changelogs and packaged crates, and explicit release authorization.
Preparation must not create tags, publish packages, or change remote settings.

### Preparation verification and remaining gates

Original integration-checkout evidence (2026-09-06; includes the separate SOCKS work):

| Gate | Result |
| --- | --- |
| Formatting; bnb + macros strict all-target/all-feature Clippy | Pass |
| Workspace tests; bnb all-feature/bytes/mock tests; SOCKS and normalization tests | Pass |
| All-feature and no-default-feature rustdocs, denied warnings | Pass |
| Rust 1.85 workspace/runtime-all-features/renamed-consumer checks; bare-metal consumer | Pass |
| Pinned public API; semver against 0.3.2 with all/default/no features | Pass |
| Both packaged archives built together; benchmark smoke | Pass |
| bnb decode fuzz, two million cases; normalization mutation tests | Pass; all four mutations caught |
| cargo-deny advisories, bans, licenses, sources | Pass; no yanked-wnaf exemption |
| Workflow syntax; FFI bridges; Rust-skills checks; blessed-version synchronization | Pass |
| IP/ARP package tests and Rust 1.85 all-feature checks; CI injection tests; full rsl facade and demo builds | Pass after explicit consumer `bnb/std` dependency features |

The approved IP/ARP consumer fix explicitly requests `bnb/std` in each manifest:
their address fields always use bnb's std-only codecs, independently of injection.
The previously failing injection CI command (`cargo test --features inject -p ip
-p icmp -p tcp -p udp -p ethernet -p arp`) and `cargo build -p rsl --features full`
now pass, as do each package's tests and `cargo build -p demos --examples`.
No protocol API, wire behavior, dependency version, or bnb default is changed.

Remaining gates:

- Workspace Clippy passes its configured CI policy, but workspace-wide `-D warnings`
  still fails on unrelated rawsock/rsl-deps diagnostics (and the normal run reports
  other protocol lint debt). The strict IP/ARP all-target/all-feature check also
  stops at the existing `ethertype` documentation lint. The documented bnb/macros warnings are resolved, not
  waived. A workspace-wide cleanup is a separate change.
- Before publishing, require the regenerated release PR to contain only the bnb
  pair, with successful CI and verified archives. Secret presence and crate ownership
  are confirmed; registry-token validity and scope still require verification.

### Isolated delivery candidate

The bnb-only delivery branch is based on `main` at `751960d` (2026-09-07).
It excludes the uncommitted SOCKS crate, protocol roadmap changes, and unrelated
documentation commits. Main already contains the IP/ARP `bnb/std` fixes and
`wnaf 0.14.1`; neither is duplicated. Main's bytemuck capability and containerized
pre-push checks are retained. The new semver and workflow-lint jobs participate in
the existing `full=true` validation path, including local pre-push checks.

PR #70 was squash-merged as `52e0d1a` under `michael-smythe`; hosted main CI and
release automation passed. The approved review-branch delivery used normal Git
hooks because `repo-guard` currently supports only direct-trunk publication.

The regenerated release PR #65 correctly selected `0.4.0` for both bnb crates, but
also scheduled netlink publication and unrelated workspace tags. The release hold
above isolates the bnb pair without hand-editing versions or release changelogs.
Netlink needs a separate version/changelog decision for its breaking move from
Tokio to synchronous APIs. Re-enable other packages only through explicit review;
do not automatically remove the hold after bnb publication.
