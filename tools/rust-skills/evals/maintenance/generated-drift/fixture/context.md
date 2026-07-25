# Fixture

A contributor changed
`generated/agent-skills/rust-protocol/references/protocol.md` directly to add a
new wire-order rule. The canonical `skills/rust-protocol/` package is unchanged.
The same stable rule ID was independently added under
`skills/rust-review/references/risk-checks.md`. Generated drift validation now
fails. No migration note, source record, or eval was added.
