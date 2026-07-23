# Releasing

Versions are **derived from [Conventional Commits](https://www.conventionalcommits.org)**
by [release-plz](https://release-plz.dev). You never hand-edit a version number.

## How it works

1. **Every commit must be conventional.** `.github/workflows/commitlint.yml` lints
   every commit in a PR against `commitlint.config.mjs`. A non-conforming message
   fails CI. (We merge/rebase real commits onto `main`, so each one is parsed.)

2. **release-plz proposes the next version.** On each push to `main`,
   `.github/workflows/release-plz.yml` opens (or updates) a **release PR** that, for
   each crate, bumps its version from the commits since its last tag and updates its
   `CHANGELOG.md`. The bump rule (pre-1.0, i.e. `0.x`):

   | commit                                   | bump          |
   | ---------------------------------------- | ------------- |
   | `fix:`                                   | patch         |
   | `feat:`                                  | minor         |
   | `feat!:` / `fix!:` / `BREAKING CHANGE:`  | minor (`0.x`) |
   | `chore`/`ci`/`docs`/`refactor`/`test`/…  | no bump       |

   The two crates use **independent versions**; if `bitsandbytes-macros` bumps,
   release-plz also bumps `bitsandbytes` (it depends on it) and rewrites the
   `version = "…"` pin in the root `Cargo.toml`.

3. **Merging the release PR cuts the release.** The `release-plz release` job then
   creates the git tag(s) — name-prefixed per crate (`bitsandbytes-v0.3.1`,
   `bitsandbytes-macros-v0.3.1`, …) — and publishes the crates that opt in to
   crates.io (see below).

**crates.io publishing is opt-in per crate.** The workspace default in
`release-plz.toml` stays `publish = false`; only `bitsandbytes` and
`bitsandbytes-macros` carry `[[package]]` overrides with `publish = true`. Merging a
release PR therefore tags every released crate but uploads only that pair to
crates.io. GitHub Releases remain disabled (`git_release_enable = false`).
(Versions up to 0.3.1 were published by hand before this automation existed.)

## Baseline tags (done)

release-plz computes a bump from the commits **since the last tag**, so the `0.1.0`
baseline is already tagged on `main` — `v0.1.0` (runtime) and
`bitsandbytes-macros-v0.1.0` (macros) — which keeps the pre-`0.1.0` history (it
predates this convention) from ever being re-scanned. If you ever need to recreate
them:

```sh
git checkout main && git pull
git tag v0.1.0                      # runtime crate (bitsandbytes)
git tag bitsandbytes-macros-v0.1.0  # macro crate
git push origin v0.1.0 bitsandbytes-macros-v0.1.0
```

## Required: a token that can open the release PR

The **RawSocketLabs org disallows GitHub Actions from creating pull requests** (org
Settings → Actions → General → Workflow permissions — it's a 409 to enable it at the
repo level). So the default `GITHUB_TOKEN` is rejected when release-plz opens the
release PR:

```
403 — GitHub Actions is not permitted to create or approve pull requests
```

A token that acts as a **user or GitHub App** (not as "GitHub Actions") is therefore
required — it isn't subject to that policy, and it also makes CI run on the release PR
(which `GITHUB_TOKEN`-opened PRs don't trigger). Add it as the `RELEASE_PLZ_TOKEN`
repository secret; the `release-plz-pr` job already prefers it
(`${{ secrets.RELEASE_PLZ_TOKEN || secrets.GITHUB_TOKEN }}`), so no workflow change is
needed. Use one of:

- **A fine-grained PAT** (simplest) — repository access to this repo, with
  **Contents: read/write** and **Pull requests: read/write**.
- **A GitHub App token** (no human owner, auto-rotated) — install an app with the same
  permissions and mint the token via `actions/create-github-app-token` in the workflow.

Until that secret exists, the `release-plz-release` (tags-only) job still works
(pushing tags needs no PR-creation rights), but `release-plz-pr` will **403** the first
time there is a releasable (`feat:`/`fix:`) commit on `main`.

Alternatively, a RawSocketLabs **org owner** can allow Actions to create PRs org-wide
(org Settings → Actions → General → "Allow GitHub Actions to create and approve pull
requests"); then the default `GITHUB_TOKEN` suffices and no PAT is needed.

## Publishing to crates.io

The `release-plz-release` job reads the `CARGO_REGISTRY_TOKEN` repository secret — a
crates.io token with publish scope for `bitsandbytes` and `bitsandbytes-macros`. The
two crates are published together and in dependency order (the runtime crate cannot
be published, or even verified, without the macro crate on the registry). If the
secret is missing when a release PR merges, the publish step fails; tags are still
created by the same run, so fix the secret and re-run the job.

To start publishing **another** workspace crate, add its own `[[package]]` entry with
`publish = true` in `release-plz.toml` — never flip the workspace default. Several
workspace crate names (`ethernet`, `arp`, `udp`, `ip`, `dns`, …) already exist on
crates.io as unrelated projects, so a blanket `publish = true` would attempt uploads
to names we do not own. Check ownership of the crates.io name first
(`cargo owner --list <name>`).
