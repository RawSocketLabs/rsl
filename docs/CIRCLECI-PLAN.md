# CircleCI migration plan

Status: cutover prepared on branch `ci/circleci-trial`, 2026-10-15. [CI.md](CI.md) describes the
resulting pipeline; this file records the decisions and what remains before and after cutover.

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
| Image publication | CircleCI `image` job, classic GHCR token (`write:packages`) in `rsl-image` | No Actions left to use the short-lived token; GHCR rejects fine-grained tokens |
| Release credentials | `RELEASE_PLZ_TOKEN` (tags, release PRs) and `CARGO_REGISTRY_TOKEN` in `rsl-release`, restricted to main | Reuses the existing release-plz token |
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

## 3. Before cutover (on this branch)

1. Publish the first image and pin its digest. The bootstrap needs the `rsl-image` context
   (a classic token with only `write:packages`); later images come from the `image` job.
2. Push the branch and run CircleCI on it with the image: a full run and a `bitsandbytes`
   release run, both green, and `scripts/ci-local.sh pre-push --full` green locally.
3. Create the `rsl-release` context, restricted to `main`, holding `RELEASE_PLZ_TOKEN` and
   `CARGO_REGISTRY_TOKEN`. Prove a branch job that references it is denied.

## 4. Cutover

1. Fast-forward `main` to the branch. This removes the Actions workflows in the same change.
2. Create the CircleCI triggers:
   - GitHub App push, `all-pushes`;
   - schedule, `main`, daily at 09:17 UTC, parameter `full=true`;
   - schedule, `main`, Mondays at 06:00 UTC, parameter `audit=true`.
3. Add a ruleset on `main` requiring `ci/circleci: CI` from the CircleCI app, and remove the
   Actions `CI` requirement.
4. On the first main pipeline, confirm `base` in the `plan` job's `ci-plan.json` artifact is the
   previous main commit, not empty. If it is empty, every main push runs full coverage and
   never qualifies a release; fix that before the next release.
5. Set the `rsl-ci` package on GHCR to public, so pulls need no credentials.

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
