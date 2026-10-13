# USDR portable-core compilation witness

This unpublished, detached validation workspace imports the **exact production**
`../src/portable` module tree. The native driver and this fixture compile the same address
types, TMP114 register values/temperature machine, Si5332 divider plan, USB wire fields and
I²C transaction machine, and the board's RF machines (LMS6002D operations, tuning and
calibration, power-up, rate, tuning, gain, path and stream composites), which the witness
drives against a register-file board. There are no copied algorithms or substitute host errors.
Where the tree re-exports the shared no_std crates (`rsl-lowlevel`'s addresses, register
traits and sequence seam; `rsl-lp8758`'s LP8758 values), the fixture depends on them by
path without default features, so it compiles exactly what the driver does minus `std`.

```sh
python3 scripts/ci/portable.py
cargo check --manifest-path devices/sdr/usdr/nostd-check/Cargo.toml --locked --offline --target thumbv7em-none-eabi
cargo test --manifest-path devices/sdr/usdr/nostd-check/Cargo.toml --locked --offline
```

Run from the repository root with the pinned toolchain and target installed. The dependency
check compares each fixture dependency's path/version/features with the actual production
manifests (a per-dependency table in `scripts/ci/portable.py`; `rsl-lowlevel`'s `std` is the
only host-only feature).
It deliberately does not depend on the whole std/native driver just to expose ownership;
the CI selector explicitly records that production-source edge.

The bare-metal gate proves **no_std + alloc compilation**, including real macro-expanded
register paths. It does not prove embedded linking, an allocator-free dependency stack,
kernel suitability, firmware deployment or physical operation. `bnb` requires alloc;
the fixed-size operation machines themselves do not allocate. The public `rsl-usdr` crate
still requires std and does not advertise no_std support.

Ordinary CI runs only this fixture's host format/lint/check/tests and dependency contract.
Bare-metal CI remains release-only and requires the native USDR product to be explicitly
released; a codec release reaching this fixture through dependencies cannot enable its
cross-target qualification. USDR release/publication remains disabled.
