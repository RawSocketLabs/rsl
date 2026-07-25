# Repository Adoption

Adoption is explicit. Merely having `tools/rust-skills` in the RSL checkout, or
exporting the directory into another repository, does not activate its skills.

## Adopt in a repository

1. Pin an exact standards release or commit.
2. Run the read-only inspector from the consumer repository:

   ```text
   /path/to/rust-skills/skills/rust-repository-onboarding/scripts/inspect_repository.py . --pretty
   ```

3. Follow the onboarding skill's adaptive interview. Confirm one base profile,
   applicable capabilities, component overlays, enabled skills, local
   conventions, and required versus optional validation.
4. Present the complete proposal and obtain approval before writing.
5. Encode approved answers using
   `skills/rust-repository-onboarding/assets/approved-answers.example.json` as a
   shape, then preview the renderer without `--write`:

   ```text
   /path/to/rust-skills/skills/rust-repository-onboarding/scripts/render_adoption.py \
     --answers /path/to/approved-answers.json \
     --target . \
     --standards 0.1.0 \
     --source exact-tag-or-commit
   ```

6. Review the preview, then repeat with `--write`. The renderer refuses
   overwrites and creates only approved repository-specific policy and
   validation material. It records the adoption date, selected adapter,
   generated-manifest SHA-256, and per-skill bundle hashes. When running from an
   exported package, pass `--manifest /path/to/generated/manifest.toml`.
7. Generate and validate the standards checkout, then install one adapter
   family with the confirmed skill list:

   ```text
   cargo xtask generate --check
   cargo xtask install --agent common --scope repo --target /path/to/consumer \
     --skills rust-core,rust-implement,rust-review,rust-api-design,rust-testing
   ```

8. Run `scripts/validate-rust.py`, discovery smoke tests, and relevant evals.
   Present the resulting diff and obtain final confirmation.

Omitting `--skills` installs all 14 canonical skills. Selective installation
keeps activation context smaller for specialized repositories. The two
`rsl-rust-*` compatibility routers are installed only when explicitly named.

Installation refuses to overwrite an existing target unless `--replace` is
explicit. `multi-agent` installation remains blocked by design. Review exact
targets before using replacement in a repository with existing instructions.
The generated repository validator uses Python 3.11 or newer and invokes command
argument arrays without a shell.

## Logical precedence

Precedence is a versioned contract with one normative statement, in the
[modular architecture](architecture.md). The runtime copy an adopting repository
actually applies ships in `rust-core`'s entry file; `templates/AGENTS.root.md`
restates it for the repository's own instructions. Do not restate the ladder
anywhere else — a fourth copy is a fourth thing to drift.

A lower layer may strengthen an unconstrained choice but must not silently
reverse a higher-precedence decision.

## Canonical source and distribution

The canonical source is `tools/rust-skills` in the
`RawSocketLabs/rsl` repository. Release it with namespaced tags such as
`rust-skills-v0.1.0`; external consumers may pin that tag, an exact RSL commit,
or an archive produced from the tagged directory. Preserve the independent
Cargo workspace, source-relative paths, explicit activation, and exact source
pin in every distribution form.

The onboarding workflow and consumer provenance format are specified in the
[modular architecture](architecture.md). Existing schema-1 consumers may remain
on the compatibility path described by the
[v0.1 migration](migrations/0.1-to-modular.md), but new adoption should use the
approval-gated modular workflow.

## Legacy adoption surface

`templates/rsl-rust-standards.toml` and `cargo xtask inspect-adoption` are the
v0.1 consumer format and are frozen there on purpose. They keep the legacy
profile names — `performance-application`, `pragmatic-application`, and
`prototype` — and select the two `rsl-rust-*` routers, so they can read an
installation made before the modular split. They are not a template for a new
repository: modular adoption records `.rust-skills/adoption.toml` through
`rust-repository-onboarding`, using the bases and capabilities in
[the architecture](architecture.md).
