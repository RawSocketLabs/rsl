# Repository Adoption

Adoption is explicit. Merely having `tools/rust-skills` in the RSL checkout, or
exporting the directory into another repository, does not activate its skills.

## Adopt in a repository

1. Pin an exact standards release or commit in
   `rsl-rust-standards.toml` using the provided template.
2. Select one default profile and only the two implemented skills.
3. Fill `templates/AGENTS.root.md` with verified repository facts. Preserve
   stronger existing instructions and document exceptions.
4. Add a nested `AGENTS.md` only where a subtree truly changes facts or policy.
5. Generate and validate this standards checkout.
6. Install one adapter family into the consumer repository. Do not install both
   common and Claude roots until the Cursor coexistence gate is resolved.
7. Run repository-local discovery smoke tests and representative evals.

Example inspection command:

```text
cargo xtask inspect-adoption /path/to/consumer
```

Example repository-scoped common adapter installation:

```text
cargo xtask install --agent common --scope repo --target /path/to/consumer
```

Installation refuses to overwrite an existing target unless `--replace` is
explicit. `multi-agent` installation remains blocked by design. Review exact
targets before using replacement in a repository with existing instructions.

## Logical precedence

Apply instructions in this order:

1. current user instruction;
2. closest repository-local instruction;
3. parent or root repository instruction;
4. repository-declared domain skill;
5. general Rust skill; and
6. general agent behavior.

A lower layer may strengthen an unconstrained choice but must not silently
reverse a higher-precedence decision.

## Canonical source and distribution

The canonical source is `tools/rust-skills` in the
`RawSocketLabs/rsl` repository. Release it with namespaced tags such as
`rust-skills-v0.1.0`; external consumers may pin that tag, an exact RSL commit,
or an archive produced from the tagged directory. Preserve the independent
Cargo workspace, source-relative paths, explicit activation, and exact source
pin in every distribution form.
