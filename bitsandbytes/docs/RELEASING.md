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
   each crate, bumps its version from the commits since its last tag and updates its
   `CHANGELOG.md`. The bump rule (pre-1.0, i.e. `0.x`):

   | commit                                   | bump          |
   | ---------------------------------------- | ------------- |
   | `fix:`                                   | patch         |
   | `feat:`                                  | minor         |
   | `feat!:` / `fix!:` / `BREAKING CHANGE:`  | minor (`0.x`) |
   | `docs:`                                  | release-eligible; inspect generated PR |
   | non-breaking `chore`/`ci`/`refactor`/`test`/… | no bump    |

   The two crates use **independent versions**; if `bitsandbytes-macros` bumps,
   release-plz also bumps `bitsandbytes` (it depends on it) and rewrites the
   `version = "…"` pin in the root `Cargo.toml`.

3. **Merging the release PR cuts the release.** The `release-plz release` job then
   creates the git tag(s) — name-prefixed per crate (`bitsandbytes-v0.3.1`,
   `bitsandbytes-macros-v0.3.1`, …) — and publishes the crates that opt in to
   crates.io (see below).

**crates.io publishing is opt-in per crate.** The workspace default in
`release-plz.toml` stays `publish = false`; only `bitsandbytes`,
`bitsandbytes-macros`, and `rsl-netlink` carry `[[package]]` overrides with
`publish = true`. Merging a release PR tags released crates but uploads only those opted in to
crates.io. GitHub Releases remain disabled (`git_release_enable = false`).
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
baseline is explicitly `0.3.2`; advance it deliberately after the next release.
The checker cannot certify proc-macro expansion or behavior: consumer/UI tests
and review remain required.

Do not recreate historical tags. The `git_only` migration remains deferred in
`release-plz.toml` until a packageable tag baseline exists.

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
crates.io token with publish scope for `bitsandbytes`, `bitsandbytes-macros`, and
`rsl-netlink`. The
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

To start publishing **another** workspace crate, add its own `[[package]]` entry with
`publish = true` in `release-plz.toml` — never flip the workspace default. Several
workspace crate names (`ethernet`, `arp`, `udp`, `ip`, `dns`, …) already exist on
crates.io as unrelated projects, so a blanket `publish = true` would attempt uploads
to names we do not own. Check ownership of the crates.io name first
(`cargo owner --list <name>`).

## Preparing 0.4.0

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
- Read-only GitHub inspection found successful existing main CI and release-plz
  runs, but they do not cover this uncommitted candidate. Repository-secret
  inspection returned HTTP 403, so credential presence/scope/validity remains
  unverified. The checkout's review branch must be reconciled with current trunk
  through an approved delivery workflow before hosted candidate CI can be proven.

### Isolated delivery candidate

The bnb-only delivery branch is based on `main` at `751960d` (2026-09-07).
It excludes the uncommitted SOCKS crate, protocol roadmap changes, and unrelated
documentation commits. Main already contains the IP/ARP `bnb/std` fixes and
`wnaf 0.14.1`; neither is duplicated. Main's bytemuck capability and containerized
pre-push checks are retained. The new semver and workflow-lint jobs participate in
the existing `full=true` validation path, including local pre-push checks.

Delivery remains review-branch based. The currently installed `repo-guard` can
verify this candidate but only publishes directly to the configured trunk; it has
no review-branch adapter. Resolve that tooling/policy mismatch explicitly before
pushing. Existing release PR #65 predates these changes and is not the 0.4 candidate.
