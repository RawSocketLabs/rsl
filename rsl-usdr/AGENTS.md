# rsl-usdr

> Inherits the workspace-root `../AGENTS.md`.

A pure-Rust driver for the Wavelet Lab uSDR (`m2_lm6_1`: LMS6002D, Si5332, LP8758, TPS6381x,
TMP114), replacing the cxx FFI crate `../usdr`. Scope is RX at parity with that crate's API,
over both the USB and PCIe transports. The driver crate itself does not exist yet; this
directory currently holds its test reference.

| Path | Crate | Role |
|------|-------|------|
| `sim/` | `rsl-usdr-sim` (unpublished, workspace member) | Board model at libusdr's `ls_op` seam: FPGA registers, SPI, I2C, virtual clock, op trace |
| `oracle/` | `rsl-usdr-oracle` (unpublished, **excluded**: compiles C) | Vendored libusdr driving `sim` through `oracle/shim/sim_plugin.c` |

## Rules

- libusdr is the reference until hardware is available. Behaviour the Rust driver ports must
  first be exercised through the oracle against `sim`, then matched.
- The vendored subset under `oracle/libusdr/` is never edited by hand. Refresh it with
  `oracle/vendor.sh <usdr-lib checkout>` at the commit in `oracle/libusdr/PIN`. Bump the PIN
  in its own change.
- The sim models chips at register level and states every transport assumption it encodes,
  with its evidence, at the point it is encoded (see the I2C readback packing in `oracle/src/lib.rs`).
- Intentional divergences from libusdr are whitelisted per test scenario, never by masking a
  register globally. The first planned one is the band-crossing fix: gate Si5332 port 3
  without cycling USYS_CTRL through READY, instead of libusdr's close/reopen.
- Register layouts in the driver use `bnb` bitfields.
- `sim` stays an unpublished crate, not a feature of the driver.

## Verify

```sh
cargo clippy -p rsl-usdr-sim --all-targets && cargo test -p rsl-usdr-sim
cargo fmt --manifest-path rsl-usdr/oracle/Cargo.toml -- --check
cargo clippy --manifest-path rsl-usdr/oracle/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path rsl-usdr/oracle/Cargo.toml
```

The oracle needs a C compiler only; regenerating the vendored subset also needs `python3`
with PyYAML.
