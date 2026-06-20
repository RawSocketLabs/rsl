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

## One-time setup: the baseline tags

release-plz computes a bump from the commits **since the last tag**. There are no
tags yet, so before the first run, tag the current `0.1.0` state on `main` so the
pre-`0.1.0` history (which predates this convention) is never re-scanned:

```sh
git checkout main && git pull
git tag v0.1.0                      # runtime crate (bitsandbytes)
git tag bitsandbytes-macros-v0.1.0  # macro crate
git push origin v0.1.0 bitsandbytes-macros-v0.1.0
```

(If you skip this, release-plz will create these tags itself on its first run, but
tagging explicitly keeps the first generated changelog from summarizing old history.)

## Optional: let CI run on the release PR

PRs opened by the default `GITHUB_TOKEN` don't trigger other workflows, so CI won't
run on the release PR. To get CI there, add a PAT or GitHub App token as the
`RELEASE_PLZ_TOKEN` repository secret; the workflow uses it when present and falls
back to `GITHUB_TOKEN` otherwise.

## When you're ready to publish to crates.io

1. Add a `CARGO_REGISTRY_TOKEN` repository secret.
2. In `release-plz.toml`, set `publish = true` (and `git_release_enable = true` for
   GitHub Releases).
3. Add `CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}` to the
   `release-plz-release` job's `env` in the workflow.
