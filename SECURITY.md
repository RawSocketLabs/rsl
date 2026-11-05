# Security policy

## Reporting a vulnerability

Report vulnerabilities privately through GitHub:
[**Report a vulnerability**](https://github.com/RawSocketLabs/rsl/security/advisories/new)
(the repository's Security tab). Please don't open a public issue or pull request for a suspected
vulnerability.

Include the affected crate and version (or commit), what an attacker can do, and a reproducer
if you have one. Reports stay private until a fix is ready and an advisory is published.

## Supported versions

Only the latest published release of each crate gets security fixes. Today that is
`bitsandbytes` and `bitsandbytes-macros` on crates.io; every other crate is unpublished and is
fixed on `main` only.

## Scope

Every crate in this repository is in scope, including the `bindings/` FFI crates. Some crates
make no production-security claim by design: `crypto` is a readable reference implementation
whose security status is recorded per algorithm (see `crypto/core/SIDE-CHANNELS.md`), and
`crypto-legacy` holds deliberately broken historical algorithms. Reports against those are still
welcome when the code behaves differently from what its documentation claims.
