# CircleCI migration plan

Status: draft, revised after adversarial review, 2026-10-15. Branch `ci/circleci-trial`.
Companion to [CI.md](CI.md), whose selection and receipt contract this plan keeps unchanged.

## 0. Why, and the decision gate

None of the goals below needs CircleCI. The selector, gate and receipt already run on Actions.
Most of what makes this plan hard (merge queue, cache scope, release credentials, required
check identity) is something Actions already provides natively. CircleCI's case is the speed
of per-job fan-out and test insights. The org also has the Blacksmith Actions runner app
installed, which offers faster runners as a `runs-on` label change.

**Gate before phase 1:** run the same full, cold selection three ways on this branch and
compare wall time, slowest job, and queue time:

| Candidate | Measured |
|-----------|----------|
| Actions, `ubuntu-24.04` (current) | not measured |
| Actions, Blacksmith 4 to 8 vCPU, with the same workspace split and nextest | not measured |
| CircleCI trial | 6 min (full, cold, before the workspace split) |

Continue with CircleCI only if it is clearly ahead on that comparison, or on something
Actions cannot offer. Otherwise apply the workspace split and nextest to Actions and stop.

## 1. Goals

| # | Goal | Evidence |
|---|------|----------|
| G1 | A change runs only the checks its files and their consumers select; prose runs none. | `plan.py` contract tests; continuation job list per run |
| G2 | Fail closed: no green required check unless every selected job ran and passed on the tested SHA, reported by the expected CI app. | gate recomputation; ruleset pins the check's app |
| G3 | Routine pipeline wall time ≤ 10 min, measured from trigger to gate including setup, policy and the gate itself; each job's run step ≤ 7 min. | per-step `timeout`; insights over ≥ 50 runs |
| G4 | Publication only for the exact main commit whose complete release coverage passed, from evidence no branch could influence. | release gate; release-candidate runs without caches |
| G5 | Tools, toolchains and fetched sources are pinned and verified. | checksums; image digest; pinned commits |
| G6 | Contributors can run the same checks locally in the same environment. | local runner uses the CI image |

Non-goals: changing what the selector chooses, replacing release-plz, hardware tests.

## 2. Pipeline shape

```
setup:  plan ── continue(selected jobs only)
ci:     policy ─┬─ checks-workspace-{lint,test,extra}
                ├─ checks-<profile> …          (each selected profile)
                ├─ fuzz-<root>-<target> …      (release only)
                └─ CI  (requires all; recomputes the plan; writes the receipt)
```

Flat fan-out. The serial overhead is plan, then policy, then the slowest check, then the gate;
G3 budgets for it. When `checks-workspace-test` p95 exceeds 5 minutes, switch it to build once
and test in parallel: `cargo nextest archive` in one job, persisted to the workspace, then
`--partition hash:i/N` under `parallelism`. A tiered lint-first layout is rejected: it delays
every passing run, and lint already fails early as its own job.

Superseded runs: turn on auto-cancel for non-default branches, as Actions cancels obsolete PR
runs.

## 3. Triggers and the merge gate

| Event | Trigger | Plan |
|-------|---------|------|
| Push to a branch | GitHub App, `all-pushes` | merge base with `origin/main` |
| Push to main | same | previous main pipeline's revision; **must be verified non-empty** |
| Merge queue | GitHub App, `pushes-to-merge-queues` (**unverified**) | merge base with `main` |
| Nightly | schedule trigger, parameter `full=true` | full routine |
| Manual | `circleci pipeline run --param …` | explicit inputs |

- **Main-push base (blocking).** Main pushes take their base from `pipeline.git.base_revision`. If
  the GitHub App leaves it empty, every main push runs as a full run, and full runs never
  qualify a release. The phase 0 check: push to main in a shadow run and read `base` in the
  `ci-plan.json` artifact. Fallback: compare against the previous main commit, `HEAD^1`.
- **Merge queue.** CircleCI tests the branch head, not the merge result. A merge queue restores
  that guarantee. It is adopted only after a scratch repository shows the CircleCI App building
  `gh-readonly-queue/*` and posting the check.
- **Merge method.** release-plz reads main's commit messages, so the queue's merge method has
  to keep Conventional Commit subjects. Either rebase-merge with per-commit lint, or squash with
  a linted PR title. This contradicts the protocols guide (squash) and commitlint (rebase); one
  decision fixes both.
- **Required check identity.** A ruleset requires the gate's check pinned to the CircleCI app.
  Matching by name alone collides with Actions' `CI` check, and any branch could spoof it. The
  App's actual check name and slug come from `gh api …/check-runs` on a trial commit.

## 4. Execution environment

**CI image.** `.circleci/image/Dockerfile` builds an image with the pinned toolchains and
components, the cargo tools, and the native packages at pinned versions. CI builds it, and it
is published to GHCR under RawSocketLabs, signed, and referenced by digest. Rebuilding is a
pull request that bumps the digest. Dependabot's `docker` ecosystem tracks the base image.

- The `RUSTUP_TOOLCHAIN` variables that `circleci.py` sets come from `rust-toolchain.toml`, pinned
  to an exact stable, rather than the literal `stable`. Otherwise pinning the file changes
  nothing in CI.
- The DSDcc headers are fetched at a pinned commit, not `--depth 1` of HEAD.
- Docker versus machine executor is decided by measurement:
  - check memory (Docker `large` has 8 GB against machine's 15 GB) on the workspace test;
  - check image pull time;
  - check the raw-socket needs of `network`;
  - actionlint runs from its binary, not `docker run`.

**Caches.** Only main saves caches, keyed by profile, toolchain and `Cargo.lock`. CircleCI caches
are project-wide, so a branch can write one. Therefore:
- **Release-candidate pipelines restore no caches at all**, routine jobs included, because their
  results authorise publication (G4).
- For other routine runs, the residual risk is accepted: only collaborators can push branches,
  forks are not built, and fork secrets are off.

**Test reporting.** Nextest's `ci` profile writes JUnit for `store_test_results`. There is no
automatic retry: a flaky test is fixed or quarantined with an issue link.

## 5. Release (CD): stays on Actions

Moving publication to CircleCI costs more than it returns:
- a long-lived crates.io token held by the CI vendor, where Actions can use short-lived OIDC
  (trusted publishing support for CircleCI unverified);
- no `GITHUB_TOKEN` for tagging;
- cross-workflow `requires` is impossible, so the receipt has to be passed through the workspace;
- release serialisation depends on an unverified `serial-group`. Checking that main still points
  at the SHA does not prevent two release jobs from overlapping.

Design: Actions keeps `release-plz.yml` and its `concurrency` group. Its trigger changes from
`workflow_run` on Actions `CI` to the CircleCI check completing on main (`check_run`/`status`
events from the CircleCI app). Its candidate job then:
1. confirms the CircleCI pipeline is for the current main SHA and that the reporting app is
   CircleCI;
2. downloads `ci-coverage.json` through the CircleCI API, using a read-only CircleCI token
   stored as an Actions secret;
3. runs `gate.py release` on it. `gate.py` stops calling `int()` on run IDs, because CircleCI
   workflow IDs are UUIDs.

Commitlint and the weekly advisory audit also stay on Actions. They have no secrets and need no
change.

## 6. Local parity

Until Actions CI is retired, `scripts/ci-act.sh` stays. After that, `scripts/ci-local.sh` runs the
plan in the CI image under rootless Podman, running `run.py <profile> [--part …]` for each
selected profile.

## 7. Migration phases

| Phase | Work | Exit criterion | Rollback |
|-------|------|----------------|----------|
| 0 Trial | Branch config; manual runs; baseline comparison (§0); verify main-push base, check names, merge-queue trigger | All three verifications recorded; full and release runs green; §0 decision taken | Delete branch |
| 1 Shadow | Merge; push and nightly triggers; Actions remains required | ≥ 50 runs. Main-push plans equal the Actions plan for the same SHA (compared by a script over both receipts). Every CircleCI-only failure has an issue. | Remove triggers |
| 2 Image | CI image, pinned toolchain file, executor measurements | Image runs reproduce phase 1 results; memory and pull time within budget | Revert to machine executor |
| 3 Gate | Ruleset requires the CircleCI check (app-pinned); merge queue on; Actions CI still runs on PRs but is not required | 1 week of merges through the queue, main green; a branch-reported fake `CI` check does not satisfy the ruleset | Re-require Actions `CI` |
| 4 Release link | `release-plz.yml` triggered by the CircleCI check and reading its receipt | Negative tests pass: superseded SHA, failed CI, and a tampered receipt all fail closed. A `release-plz --dry-run` succeeds from a CircleCI receipt. | Revert the trigger to Actions CI |
| 5 Retire CI on Actions | Remove `ci.yml` and act tooling; update `plan.py` central paths, `CI.md` and `AGENTS.md` | Policy tests pass; documented admin bypass for a CircleCI outage | Restore `ci.yml` and re-require it |

## 8. Risks

| Risk | Mitigation or evidence |
|------|------------------------|
| Main-push base empty under the GitHub App, blocking release qualification | Phase 0 verification; `HEAD^1` fallback |
| Branch-written cache influences release evidence | Release-candidate pipelines without caches |
| Required check spoofed or colliding with Actions | App-pinned ruleset; phase 3 negative test |
| Merge queue unsupported for the CircleCI App | Scratch-repo test before phase 3; otherwise keep testing branch heads and accept merge skew |
| Plan concurrency limit queues a release fan-out of about 30 jobs | Check the plan's limit; measure queue time |
| Docker executor out of memory or slow to pull | Phase 2 measurements; machine executor fallback |
| nextest exposes shared-state tests | Trial full run; quarantine with an issue |
| Setup lists drift across `ci.yml`, `circleci.py` and the image | Test comparing them until phase 5 |
| CircleCI outage blocks merges | Admin bypass documented; Actions CI restorable from history |

## 9. Open decisions

Taken one at a time:
1. §0: CircleCI or Actions on faster runners, decided from the three-way comparison.
2. Merge method under a merge queue (rebase with per-commit lint, or squash with a linted title).
3. Whether `rust-toolchain.toml` pins an exact stable version (recommended either way).
4. Whether fork PRs should get a required check (the repository is public).

Not yet covered: SBOM and build-provenance attestation for published crates, and rulesets kept as
code.
