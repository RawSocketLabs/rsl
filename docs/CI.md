# CI coverage and delivery

CI runs on CircleCI: complete affected-package and downstream-consumer suites for every pushed
branch and for main. It does
not choose individual Rust functions or tests. Building an affected consumer's dependencies
is expected; running every unrelated dependency's own tests is not.

## Selection contract

`scripts/ci/plan.py` emits deterministic schema-2 JSON: compared revisions, affected package
roots/names, profiles, release products/members/versions, qualification scope, fuzz targets/budgets,
full/release modes, and reasons. Cargo metadata
supplies all declared owned dependency edges, including optional, dev, build and other-platform
dependencies. Reverse reachability uses both old and tested trees, so removed edges still select
old consumers. Input paths come from NUL-delimited Git diffs, without forge pagination limits.

Branches compare from their merge base with `origin/main` and test the pushed commit. Main pushes
compare against the previous main pipeline's revision; without one they run full coverage. Manual runs without a base, missing comparison history,
shared toolchain/build policy, and central CI changes select full routine coverage, never
release qualification. Invalid metadata is an error, not an empty plan. A changed unregistered Cargo manifest
fails until its validation is registered; deleting an owned package selects full coverage.

Detached fuzz/no_std/FFI/tooling workspaces are registered in the selector. Additional detached
members require explicit validation registration; rust-skills is validated as its whole
workspace. Its evaluation manifests and generated Markdown are inputs, not prose exemptions.
Shared PKI vectors select the certificate-stack consumers' routine checks.

Scope and intensity are independent. Ordinary branches, main pushes, local pre-push, and nightly
full runs perform selected package tests and lint plus relevant feature/FFI smoke checks.
One all-feature lint/test configuration keeps optional codec I/O, SOCKS adapters and USDR
transports covered even when default features are empty. They do not run fuzzing,
packaging, public-API/semver qualification, cross-target/no_std,
MSRV, or exhaustive feature ladders. Shared files and missing history can widen routine
scope, but cannot enable these release-only jobs.

Dependency-only edits to root `workspace.dependencies` are narrowed using Cargo's expanded
declarations, including renamed, optional, dev/build and platform-specific dependencies.
Resolver, membership, workspace-package metadata, lints, patches, profiles and other root
manifest changes retain full routine coverage. Member manifest changes retain their owners
and consumers, adding shared dependency-feature users where necessary.

Lockfile deltas compare each package's transitive locked closure, including owned/path nodes,
versions, sources, dependency edges and checksums. Each snapshot uses Cargo's
`workspace_root` lockfile; detached workspaces remain independent. Declaration and locked-code
changes also select possible transitive feature-sharing peers; declaration changes cross
every owned consuming workspace context, while lock-only effects use the edited resolution context.
Multiple locked versions/sources of one declaration name are unioned conservatively.
Missing/unsupported relevant lock information falls back to full routine coverage; unrelated
unchanged ignored locks do not defeat narrowing. Lockfile-only unreachable-node/reordering
changes retain dependency audit without retesting unaffected packages.
Root build/check/lint/test/doc commands use `--locked`: a stale manifest/lockfile pair
fails rather than resolving new, unplanned dependencies after scope selection. Detached
profiles with intentionally ignored lockfiles retain their existing lock policy.

Release products come from enabled `release-plz.toml` packages, grouped by `version_group`
(or package name). Trusted release PRs/merges infer products from changed manifest versions:
every group member must change version and share the new version. Qualification covers the
product, owned forward dependencies, and affected downstream consumers. Dependencies do
not pull in their unrelated consumers. Heavy scope is computed independently of routine
changed-file scope: even a root lockfile or CI edit cannot add unrelated release jobs.
Currently only the bitsandbytes runtime/macros product is enabled; no release opt-ins change here.

The USDR portable-core fixture is a registered detached consumer of both its bnb dependency
and the production source tree it imports through `#[path]`. Its `usdr-portable` routine
profile runs the source/dependency consistency contract and host fmt/Clippy/check/tests.
It installs no bare-metal target and performs no cross-target qualification. A future
explicitly enabled/releasing USDR product additionally requires its registered fixture and
selects its thumbv7em-none-eabi compile gate in the release-only `no-std` profile. A codec
release can reach this fixture as a downstream consumer without enabling that USDR target.
The fixture also depends, without default features, on the shared no_std device crates
`hal` and `lp8758`; `scripts/ci/portable.py` holds each portable-core dependency
to the production declaration minus host-only features (`hal`'s `std`). Those crates
declare `#![no_std]` themselves, and their own thumbv7em-none-eabi `cargo check` (default
features) joins the same release-only `no-std` profile when a driver with a portable fixture
is explicitly released (`plan.BARE_METAL_DEVICES`, `plan.PORTABLE_FIXTURES`); a codec release
does not select it.

The LMS7 family driver's fixture (`devices/sdr/xsdr/nostd-check`, profile `xsdr-portable`)
follows the same contract for `xsdr`'s `src/portable` tree, on `hal`,
`lp8758` and `lms7002m` (`portable.py xsdr`); `lms7002m` is a registered
bare-metal device crate. No XSDR product is release-enabled, so its cross-target checks are
registered but not selected today.

The LSDr driver's fixture (`devices/sdr/lsdr/nostd-check`, profile `lsdr-portable`) does the
same for `lsdr`'s `src/portable` tree, on `bnb` and `hal` (`portable.py lsdr`);
its chips' register values live in that tree, so it adds no device crate. No LSDr product is
release-enabled either. Its oracle runs as `lsdr-oracle`, which a change to `lsdr`
selects too (the oracle's parity tests depend on the driver).

The LimeSDR-mini driver's fixture (`devices/sdr/limesdr/nostd-check`, profile
`limesdr-portable`) does the same for `limesdr`'s `src/portable` tree, on `hal`
and `lms7002m` (`portable.py limesdr`); no LimeSDR product is release-enabled. Its oracle
runs as `limesdr-oracle`, which a change to `limesdr` or `lms7002m` selects too.

The PE_SYNC driver's fixture (`devices/sync/pe_sync/nostd-check`, profile `pe-sync-portable`)
does the same for `pe-sync`'s `src/portable` tree, on `bnb`, `hal` and
`lmk05318` (`portable.py pe-sync`; portable profiles take the driver directory's name with
dashes); `lmk05318` is a registered bare-metal device crate. No PE_SYNC product is
release-enabled. Its oracle runs as `pe-sync-oracle`, which a change to `pe-sync` or
`lmk05318` selects too.

All five device oracles compile the one vendored libusdr tree, `devices/vendor/libusdr`
(refreshed by `devices/vendor/vendor.sh`); a change under `devices/vendor/` selects every
oracle profile and no driver package. The FFI bindings' compile check runs as `usdr-ffi`.

| Change | Coverage beyond lightweight policy validation |
|---|---|
| Ordinary allowlisted prose | No Rust setup, builds, or fuzzing |
| SOCKS | SOCKS default/all-feature tests/lint; no upstream codec tests or fuzzing |
| Compression | Compression and facade tests/builds |
| Crypto | Crypto, legacy crypto and affected PKI/facade tests/lint |
| bnb/macros | Codec and affected protocol/PKI/device consumer tests/lint |
| Dependency-only manifests/lockfiles | Changed dependency users and old/new consumers; shared feature users where needed |
| Shared build policy/central CI | Full routine workspace/detached checks; no heavy jobs |
| Product release | Relevant fuzzing, feature/MSRV/no_std matrices, compatibility and product packaging |

The prose exemption is deliberately narrow: known README/CHANGELOG/DESIGN/ROADMAP files,
the explicitly approved USDR `PORTING.md` capability ledger,
reviewed documentation trees/assets, and licenses, excluding fixtures/tests/fuzz and generated
skills. `.rs` edits remain code even if only rustdoc changed. Unknown paths expand to full
coverage. A domain gate edit such as `.github/ci/bnb.toml` selects that domain and consumers.

## Execution and local commands

### Editing versus qualification

Ordinary local editing uses focused package/behavior tests, formatting, and strict package
Clippy with the relevant features. Prose-only edits need diff review and whitespace checks;
discussion needs no checks. Module/feature changes add relevant configurations, not an
exhaustive matrix. See the root `AGENTS.md` and the affected crate's routine tier.

Full local workspace/feature/MSRV/release-profile suites, fuzzing, mutation testing, and
benchmarks are reserved for release preparation or an explicit request for broader
qualification. Do not launch them automatically after each edit or rerun unchanged inputs.
This local cadence does not modify the selector, pre-push hooks, hosted PR/main gates,
scheduled coverage, or release provenance requirements described here. Publication still
requires its selected checks; ordinary editing does not invoke publication gates.

### CI tooling and pre-push

```sh
python3 -m unittest discover -s scripts/ci -p 'test_*.py' -v
python3 -m unittest discover -s scripts/ci -p 'integration_*.py' -v
python3 scripts/ci/plan.py --base origin/main
scripts/ci-local.sh pre-push
scripts/ci-local.sh pre-push --base REF
scripts/ci-local.sh pre-push --full
scripts/ci-local.sh release --product bitsandbytes
```

Fast policy tests use only Python's standard library and Git. Full-tier selector integration
tests exercise actual Cargo metadata without compilation/network, including other-platform
and optional dependency declarations. The fixed profile adapter is `scripts/ci/run.py`;
it consumes the plan through `CI_PLAN`, not arbitrary shell commands from changed filenames.
Jobs are handed the plan without its per-path detail (`paths`, `prose`, `reasons`,
`full_reasons`), which grows with the diff: Linux caps one environment variable at 128 KiB, and
the generator refuses a handed plan past that. The detail stays in the plan job's artifact and
the coverage receipt; the final job receives only the other jobs' results, not their outputs.

`scripts/ci-local.sh` runs CircleCI's pipeline on the local machine under rootless Podman, in the
same pinned `rsl-ci` image. The plan job's generator writes the continuation CircleCI would run,
and each job's own `run` steps execute unchanged in a fresh clone of the tested commit: `policy`
and `commitlint` first, then the selected checks and fuzz targets (`-j N`, default
`CI_LOCAL_JOBS` or 2), then `CI`, which recomputes the plan and rejects divergence. Release jobs
never run locally; a changed job image is built, not published. A machine job (`commitlint`)
runs its script on the host with Podman standing in for Docker, so its own container does the work. It requires a clean committed
tree and tests a self-contained clone, so linked worktrees and unstaged files never reach it.
The default comparison is the merge base against the trunk recorded by repo-guard's local
configuration; absent configuration/history means full routine coverage, and no remote is
fetched to choose it. `--image` runs an unpublished image (for example `localhost/rsl-ci:trial`
built from `.circleci/image/`). Build targets and the Cargo registry are cached under
`~/.cache/rsl-ci-local`, one job at a time per cache. Each run's clones and logs live under
`~/.cache/rsl-ci-local/runs`; a passing run removes its directory, a failing one keeps it.

## Pipeline

`.circleci/config.yml` is a setup workflow: its `plan` job runs the selector through
`scripts/ci/circleci.py` and continues the pipeline with only the selected jobs. Unselected
profiles are never scheduled; a prose-only change runs `policy`, `commitlint` and the `CI` gate.
Selected jobs all start once `policy` passes and run concurrently. The `workspace` profile, the
critical path, runs as three jobs (`run.py --part`): `lint` (fmt and Clippy), `test` (`cargo
nextest run`, which runs every test binary at once, plus `cargo test --doc`), and `extra`
(all-feature adapter tests and selector integration tests). Together they are exactly the
profile's commands over the same packages and features, so full runs still test workspace
feature unification. Nextest's `ci` profile reports JUnit to CircleCI's test insights. Every job
is bounded by a 10-minute budget.

Fuzz targets run as separate jobs and processes: bnb retains 60 seconds/2,000,000 runs per
target, crypto/PKI retain 45 seconds/500,000 runs, and SOCKS runs the documented 2,000,000 cases
with max length 2048. Limits retain libFuzzer's original stop-when-either-limit-is-reached
behavior. Relevant feature ladders, renamed no_std compilation, strict codec/SOCKS lints, FFI
bridge checks, and pinned public-API/compatibility gates are not replaced with compile-only
approximations.

Every job runs the `rsl-ci` image (`.circleci/image/Dockerfile`), pinned by digest in the
generator and the setup config: its toolchains (pinned stable, MSRV, and the pinned nightly for
public-API and fuzzing), cargo tools, release-plz, native libraries and DSDcc headers are fixed at
build time, and nothing is installed at run time. A change under `.circleci/image/` adds an
`image` job to a main push, after `CI` passes, that builds and publishes
`ghcr.io/rawsocketlabs/rsl-ci:<sha>` with the `images` context's GHCR token; adopting it is a
reviewed digest change. To try a changed image first, build it locally and run
`scripts/ci-local.sh pre-push --image localhost/rsl-ci:<tag>`. `test_circleci.py` keeps the
Dockerfile pins, `bnb.toml`, the generator and the setup config in step. `commitlint` lints every
commit since the base with the pinned upstream commitlint image and `.config/commitlint.config.mjs`.

Event mapping: main compares against `pipeline.git.base_revision`. When that is absent (`<nil>`,
always so for GitHub App pipelines) main runs full coverage, because an admin push can bypass the
branch run, and judges releases against the pushed head's first parent, as the release gate does;
a version bump below the tip of a multi-commit push qualifies nothing. Main qualifies any release whose versions it
changes; any other branch compares from its merge base with `origin/main`, and a same-repository
`release-plz-` branch qualifies its products. Any explicit parameter (`full`, `base`, `release`,
`products`) is a manual run: the nightly schedule passes `full=true`, and the weekly advisory
schedule passes `audit=true`, which continues with only `cargo deny check advisories`. A
namespaced `resource_class` parameter selects a self-hosted container runner with the same image.

Caches are keyed by job, toolchain, job image and the job's lockfile (the root `Cargo.lock`, or a
fuzz workspace's own). Each key is written once, from a clean build, and only by a
full-coverage main pipeline, so a scoped push cannot store a partial build. Detached
workspaces' own lockfiles are not in the key. Jobs build with `CARGO_INCREMENTAL=0` and
line-tables-only debuginfo to keep cached targets small. CircleCI caches are
project-wide, not branch-scoped, so a pipeline whose evidence can authorise publication (any
release qualification) restores and saves no cache at all. Forked pull requests are not built.

## Required gate and release boundary

Changes reach main by fast-forward after their branch is green: the required status is
`ci/circleci: rsl`, created by the CircleCI GitHub App (`circleci-app[bot]`), so the pushed commit
already carries it. A repository ruleset requires it with no bypass actors. A manual run with an
explicit `base` names its gate `rsl-scoped`, so a narrowed run never reports the required status.
The `rsl` gate job recomputes the plan from the pipeline's inputs and immutable
trees, then requires every generated job: CircleCI starts it only after all of them succeeded,
so a failed or cancelled job leaves it pending, never green. An empty documentation plan is
legitimate only after successful policy/selection verification.

The gate writes `ci-coverage.json` (the plan and job results, bound to the exact tested SHA and
the workflow ID) as an artifact and to the workspace. On a main push, `release-pr` refreshes the
release-plz PR on every pipeline, and a release-qualifying push adds `release` before it. Both
hold only the contexts they use (`release-pr`: `github`; `release`: `github` and
`crates`) and share one serial group, so tag discovery never races a release. Every context
is restricted to this project (`images` is shared with libsdr) and
`pipeline.git.branch == "main" and not job.ssh.enabled`, so
neither a branch pipeline nor an SSH rerun can read a secret. `release` stops successfully unless main still points at the tested commit and
the commit is the exact merge of a same-repository `release-plz-` PR. It then verifies the same
workflow's receipt: the publisher independently recomputes released members from the commit's
first parent and exact manifest versions, and requires the receipt's products, members and
qualification scope to match, before `release-plz release` runs. Manual `release=true` infers
version changes from the comparison base; explicit `products` requests enabled products even
without version changes. Neither authorizes publication. Release-plz still owns version
selection, package allowlisting, PR discovery, and publication order.

Recovery: a receipt is bound to its workflow ID, and "Rerun from failed" starts a new
workflow whose `release` job would reject the earlier receipt. After a flaky failure on main,
use "Rerun workflow from start". Redundant branch workflows are auto-cancelled; main's are not.

No success is reused across different SHAs. Dependency edits use proven declaration/resolution
deltas; shared policy or unavailable dependency-impact evidence retains the conservative
full-routine fallback. Release-only qualification remains limited to the releasing product's closure.

## Measurements

The last Actions reference main run (`34317551178` at `436e3756`) took 308 seconds with serial
crypto fuzzing as its critical path. On CircleCI, the full routine selection (19 profiles, cold
caches) took 3m37 after the workspace split; a `bitsandbytes` release run (37 jobs, 7 fuzz
targets, cold) took 7m50, bounded by the SOCKS fuzz budget. Timing must distinguish queue delay,
wall time, summed job time, and cold/warm caches.

Acceptance: ordinary prose schedules zero Rust/fuzz profiles; SOCKS does not test upstream bnb
or unrelated crypto; ordinary/full/nightly runs schedule zero heavy profiles; product releases
retain relevant downstream fuzzing and qualification; complete full-routine gates remain
reachable. Existing workspace warn-level lint debt is reported separately.
