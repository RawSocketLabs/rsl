# CI coverage and delivery

CI runs complete affected-package and downstream-consumer suites on PRs and main. It does
not choose individual Rust functions or tests. Building an affected consumer's dependencies
is expected; running every unrelated dependency's own tests is not.

## Selection contract

`scripts/ci/plan.py` emits deterministic schema-1 JSON: compared revisions, affected package
roots/names, profiles, fuzz targets/budgets, full/release modes, and reasons. Cargo metadata
supplies all declared owned dependency edges, including optional, dev, build and other-platform
dependencies. Reverse reachability uses both old and tested trees, so removed edges still select
old consumers. Input paths come from NUL-delimited Git diffs, without forge pagination limits.

PRs compare from their merge base and test the checked-out merge revision. Main pushes compare
the event's before/after revisions. Manual runs without a base, missing comparison history,
shared root manifests/lockfiles/toolchain/build policy, and central CI changes select full
coverage. Invalid metadata is an error, not an empty plan. A changed unregistered Cargo manifest
fails until its validation is registered; deleting an owned package selects full coverage.

Detached fuzz/no_std/FFI/tooling workspaces are registered in the selector. Additional detached
members require explicit validation registration; rust-skills is validated as its whole
workspace. Its evaluation manifests and generated Markdown are inputs, not prose exemptions.
Shared PKI vectors select the certificate-stack consumers and PKI fuzzing.

| Change | Coverage beyond lightweight policy validation |
|---|---|
| Ordinary allowlisted prose | No Rust setup, builds, or fuzzing |
| SOCKS | Default/blocking/Tokio/all-feature suites, docs, strict Clippy, MSRV, session fuzz |
| Compression | Compression and facade tests/builds |
| Crypto | Crypto, legacy crypto, PKI consumers; crypto/PKI fuzz |
| bnb/macros | Codec feature/UI/API/compatibility/no_std gates, protocol/PKI consumers and fuzz |
| Root lockfile/shared build policy/central CI | Full workspace and detached validation |

The prose exemption is deliberately narrow: known README/CHANGELOG/DESIGN/ROADMAP files,
reviewed documentation trees/assets, and licenses, excluding fixtures/tests/fuzz and generated
skills. `.rs` edits remain code even if only rustdoc changed. Unknown paths expand to full
coverage. A domain gate edit such as `.github/ci/bnb.toml` selects that domain and consumers.

## Execution and local commands

```sh
python3 -m unittest discover -s scripts/ci -p 'test_*.py' -v
python3 -m unittest discover -s scripts/ci -p 'integration_*.py' -v
python3 scripts/ci/plan.py --base origin/main
scripts/ci-act.sh pre-push
scripts/ci-act.sh pre-push --base REF
scripts/ci-act.sh pre-push --full
```

Fast policy tests use only Python's standard library and Git. Full-tier selector integration
tests exercise actual Cargo metadata without compilation/network, including other-platform
and optional dependency declarations. The fixed profile adapter is `scripts/ci/run.py`;
it consumes the plan through `CI_PLAN`, not arbitrary shell commands from changed filenames.

Local pre-push requires a clean committed tree. Its default comparison is the merge base
against the trunk recorded by repo-guard's local configuration; absent configuration/history
means full coverage. Explicit `--base` selects a different comparison. No remote is fetched
implicitly to choose that base. Linked worktrees use a retained self-contained temporary clone
because act cannot follow their external Git-directory pointers; its path is printed. That
clone is validation evidence, not a publication checkout. `ACT_CONCURRENT_JOBS` defaults to 2.

Hosted profiles run independently. Fuzz targets use separate processes/matrix entries with
at most four concurrent entries; bnb retains 60 seconds/2,000,000 runs per target, crypto/PKI
retain 45 seconds/500,000 runs, and SOCKS runs the documented 2,000,000 cases with max length
2048. Limits retain libFuzzer's original stop-when-either-limit-is-reached behavior. Full mode
keeps `cargo test --workspace` so workspace feature unification remains tested. Relevant feature
ladders, renamed no_std compilation, strict codec/SOCKS lints, FFI bridge checks, and pinned
public-API/compatibility gates are not replaced with compile-only approximations.

Full CI runs daily at 09:17 UTC and can be dispatched manually. The independent weekly
advisory audit remains active for newly disclosed advisories on unchanged dependencies.
Only obsolete PR CI runs are cancelled; main and publisher runs are not.

## Required gate and release boundary

The required branch-protection context stays `CI`. The final job recomputes the plan from
the actual event and immutable trees, then requires successful planning/policy checks and
every selected check/fuzz matrix. An unexpected skip, failure or cancellation is red. An
empty documentation plan is legitimate only after successful policy/selection verification.

The successful gate uploads `ci-coverage-<run-id>-<attempt>` containing the plan and job
results, bound to the exact tested SHA. Receipts expire after 30 days. The release workflow
accepts only successful push CI from this repository's configured workflow on current main;
it independently verifies the exact merged release-PR association. A read-only job checks the
same run/attempt's artifact, outside the executable checkout, and verifies complete released-
package/downstream coverage before any publishing job can start. Manual `release=true` adds
checks only; it never establishes release-candidate provenance. Superseded commits and
missing/invalid receipts do not publish. Release-plz still owns version selection, package
allowlisting, PR discovery, and publication order; tokens and publisher serialization are unchanged.

No success is reused across different SHAs. Root dependency changes still select full CI on
release PRs; dependency-resolution-delta analysis is intentionally deferred.

## Rollout and measurements

The shadow checkpoint keeps legacy gates authoritative while reviewing the selector. The
reference hosted main run `34317551178` at `436e3756` took 308 seconds; serial crypto fuzzing
took 256 seconds and was the critical path. Those jobs overlap: summing their times is not
wall time. Candidate timing must distinguish queue delay, workflow wall time, summed runner
usage, and cold/warm caches; sharding can trade runner overhead for shorter latency.

Acceptance: ordinary prose schedules zero Rust/fuzz profiles; SOCKS does not test upstream bnb
or unrelated crypto; bnb/crypto changes cover PKI fuzzing; complete full-mode gates remain
reachable. Independent reviewer approval, selector/command tests, focused omission mutations,
actionlint, shell checks, full container validation, and hosted CI are delivery requirements.
Existing workspace warn-level lint debt is reported separately. Historical bnb-macro warnings
and the yanked wnaf 0.14.0 blocker were already resolved and must not excuse new failures.
