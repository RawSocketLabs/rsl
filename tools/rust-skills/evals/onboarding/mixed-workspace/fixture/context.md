# Observed repository

- Root Cargo workspace with `codec`, `daemon`, `firmware`, and excluded
  `vendor-sys` crates.
- `codec` is a public binary-protocol library with known-answer vectors.
- `daemon` uses an async runtime and bounded channels.
- `firmware` is `no_std` and targets Cortex-M.
- `vendor-sys` contains a build script and unsafe FFI bindings.
- Root and firmware directories already contain different `AGENTS.md` files.
- CI tests the root workspace but not firmware or FFI.
- No repository MSRV or unsafe policy is documented.
