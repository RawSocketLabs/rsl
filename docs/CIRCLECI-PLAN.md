# CircleCI migration plan

Status: cut over 2026-10-15; §4 step 4 awaits one more push. [CI.md](CI.md)
describes the resulting pipeline; this file records the decisions and what remains after cutover.

## 1. Decisions

| Decision | Choice | Reason |
|----------|--------|--------|
| CI platform | CircleCI replaces GitHub Actions entirely | Fan-out and test insights; one environment for CI and local runs |
| Pipeline shape | Flat fan-out after `policy`; `workspace` split into three jobs | Lowest wall time; lint fails early as its own job |
| Environment | `rsl-ci` image pinned by digest; nothing installed at run time | Reproducible runs; removes per-job setup (4m25 for semver-checks alone) |
| Local runs | `scripts/ci-local.sh` runs CircleCI's generated jobs in the same image | Replaces act, which ran a different environment |
| Path to main | Fast-forward after the branch's `ci/circleci: CI` is green | Trunk-based; the tested commit is exactly what lands, so no merge queue |
| Required check | `ci/circleci: CI`, pinned to the CircleCI app | Statuses are created by `circleci-app[bot]` (verified on `810df77f`) |
| Fork PRs | Not built | Public repo; a fork's config could write project-wide caches |
| Image publication | CircleCI `image` job after `CI` on a main push, classic GHCR token (`write:packages`) in `images` | No Actions left to use the short-lived token; GHCR rejects fine-grained tokens |
| Release credentials | `RELEASE_PLZ_TOKEN` in `github` (release-pr, release), `CARGO_REGISTRY_TOKEN` in `crates` (release only) | Least privilege per job; reuses the existing release-plz token |
| Context restrictions | This project, and `pipeline.git.branch == "main" and not job.ssh.enabled` | Branch pipelines and SSH reruns never read a secret |
| Release serialisation | One CircleCI `serial-group` for `release` and `release-pr` | Replaces Actions' concurrency group (`serial-group` validated by the CLI) |
| Caches | Main saves; any release-qualifying pipeline restores none | Project-wide caches must not feed publication evidence |
| Commit lint | Pinned upstream commitlint image, run by a machine job | Same configuration and semantics as before |

## 2. Measurements

| Run | Result |
|-----|--------|
| Full routine selection, machine executor, cold, before split | 6m00 |
| Same, after the workspace split and nextest | 3m37 |
| `bitsandbytes` release (37 jobs, 7 fuzz targets), cold | 7m50 (SOCKS fuzz budget is the critical path) |
| Heaviest job memory | 1.5 GiB peak, CPU saturated at 4 cores |

## 3. Before cutover (done)

1. Done: the first image was published once from the verified local build
   (`sha256:232f9589…`, tag `bootstrap-7be05951`) and pinned. Later images come from the `image`
   job, which needs `images` filled with a classic token holding only `write:packages`.
2. Done: on `e2a95323`, a full run, a `bitsandbytes` release run (37 jobs, 7m04) and
   `scripts/ci-local.sh pre-push --full` were green.
3. Done: org-wide contexts `github`, `crates`, `images` (also libsdr) and `images-pull` (libsdr,
   read-only, any branch) replace the `rsl-` ones, which CircleCI cannot rename. Denial was
   proven three ways: a branch job referencing them was `unauthorized`, an SSH rerun of main's
   `release-pr` was `unauthorized`, and the API refuses a pipeline whose config branch differs
   from its checkout branch (400).

Also done: auto-cancel of redundant branch workflows, and the project marked open source.

## 4. Cutover

1. Done: `main` fast-forwarded, removing the Actions workflows.
2. Done: the CircleCI triggers (schedules attributed to the system actor):
   - GitHub App push, `all-pushes`;
   - schedule, `main`, daily at 09:17 UTC, parameter `full=true`;
   - schedule, `main`, Mondays at 06:00 UTC, parameter `audit=true`.
3. Done: `main` branch protection requires `ci/circleci: CI` from the CircleCI app (302869)
   instead of the Actions `CI`.
4. Open: the first GitHub App push to main (`8f7f8697`) planned with `base` empty
   (`pipeline.git.base_revision` rendered `<nil>`); the previous main pipeline was API-triggered.
   A second push (`4775f4c1`) confirmed it: GitHub App pipelines never set it. Main now keeps
   full coverage and takes the pushed head's first parent as the release base; confirm `base` on
   the first push after that change. The first main pipeline also showed
   that a trunk push without a version change failed `plan`; `8f7f8697` fixed that.
5. Done: the `rsl-ci` package on GHCR is public (anonymous manifest pull returns 200).

## 5. After cutover

- First release through CircleCI: run `release-plz release --dry-run` from a release-PR merge
  before the live one. Confirm tags are pushed with `RELEASE_PLZ_TOKEN`, and that a superseded
  SHA, a failed `CI`, and a tampered receipt each stop before publishing.
- Outage procedure: an admin can bypass the ruleset to land a fix while CircleCI is down. The
  Actions workflows remain in history.
- Not yet covered: SBOM and build-provenance attestation for published crates, and rulesets
  kept as code.

## 6. Risks

| Risk | Mitigation or evidence |
|------|------------------------|
| Main pushes get no base under the GitHub App, blocking release qualification | Cutover step 4 |
| A branch-written cache influences release evidence | Release-qualifying pipelines use no caches |
| Required status spoofed | Ruleset pins the CircleCI app |
| release-plz git or API behaviour differs outside Actions | Dry-run release first; tags use the token-authenticated remote |
| Long-lived tokens in CircleCI | Contexts restricted to main or this project; forks never built; tokens scoped to their single use |
| Docker executor slower or out of memory on some job | 1.5 GiB measured peak; a machine executor is a one-line fallback |
| CircleCI outage blocks merges | Documented admin bypass |
