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
   creates the git tag(s) — `v0.2.0` for the runtime crate `bitsandbytes` (the one
   users depend on, so it gets the headline tag) and `bitsandbytes-macros-v0.2.0`
   for the macro crate.

**No crates.io publishing happens yet** — `release-plz.toml` sets `publish = false`
and `git_release_enable = false`. Today this is version + `CHANGELOG` management
plus git tags only.

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

## When you're ready to publish to crates.io

1. Add a `CARGO_REGISTRY_TOKEN` repository secret.
2. In `release-plz.toml`, set `publish = true` (and `git_release_enable = true` for
   GitHub Releases).
3. Add `CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}` to the
   `release-plz-release` job's `env` in the workflow.
