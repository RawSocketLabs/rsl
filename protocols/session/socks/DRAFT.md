# DRAFT — socks (unreviewed)

Migrated verbatim (with history) from `asyio-tools/protocols`
(session/socks, source faa9250). A from-scratch SOCKS4/4A/5 proxy
(client + server, auth, CONNECT/BIND/UDP ASSOCIATE) — the most complete of
the migrated drafts, but still unreviewed.

**Status: draft.** Excluded from the workspace (`Cargo.toml` `exclude`); does
not build or ship. Requires significant review — API, `unsafe` audit, tests,
and workspace/dependency wiring — before promotion to a workspace member.
