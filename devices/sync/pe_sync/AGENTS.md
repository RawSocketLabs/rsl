# PE_SYNC clock-distribution board (`pe_sync`)

> `CLAUDE.md` is a symlink to this file. Inherits `../../AGENTS.md` (devices) and the
> workspace-root `../../../AGENTS.md`. The uSDR ledger (`../../sdr/usdr/PORTING.md`, row
> "PE_SYNC clock tree / distribution / synchronization") governs the port programme; this
> file holds the board's own facts.

A pure-Rust port of libusdr's PE_SYNC device (`device/pe_sync/pe_sync.c`): a
clock-distribution and synchronization board, **not an SDR sample source** (no channels,
no streams; a sample-rate set is refused with `-EINVAL`). Chips: LMK05318B network
synchronizer, two LMX2820 synthesizers, LMX1204 clock buffer/multiplier, LMX1214 LO
buffer, two LMK1D1208I LVDS fan-out buffers, LP87524 PMIC, and AD5662 DACs libusdr does
not drive at open. The driver `pe-sync` ports libusdr's whole open and close, operation
for operation (see [PORTING.md](PORTING.md)); it has no transport yet. Nothing here is
hardware-qualified.

| Path | Crate | Role |
|------|-------|------|
| `.` | `pe-sync` (unpublished, root workspace member) | The driver, on `hal` and `lmk05318` |
| `sim/` | `pe-sync-sim` (unpublished, root workspace member) | Board model at libusdr's `ls_op` seam: FPGA GPO/GPI registers, the SPI and I2C clock chips, virtual clock, op trace |
| `oracle/` | `pe-sync-oracle` (unpublished, own workspace: compiles C) | libusdr's `pe_sync` driving `sim` through `oracle/shim/sim_plugin.c`; also the LMX chips' C drivers called directly |
| `nostd-check/` | `pe-sync-nostd-check` (unpublished, own workspace) | Exact-source `no_std` witness for `src/portable` |
| `../../chips/lmk05318` | `lmk05318` (unpublished, root workspace member) | The LMK05318's plan, staging map and register types, shared with `usdr` (extracted from it 2026-10-13) |

## Driver layout

Mirrors `xsdr` (`../../sdr/xsdr/AGENTS.md`):

- `src/device.rs`: `Device`, the public API: `open` (power-on, status check, clock tree;
  power-off on failure) and `close`. It owns the bus (`Box<dyn Bus>`) and runs the portable
  sequences with `hal`'s blocking `run_sequence`, in `pe_sync_configure`'s order and
  with its failure rules (which reads libusdr only logs, which failures it ignores).
- `src/lmk05318.rs`: the LMK05318's bus traffic (identity, register load, soft reset, APLL
  lock waits, sync) as blocking code on `lmk05318`: its 16-bit register pointer and
  four-byte identity read do not fit the `Sequence` seam. `usdr`'s
  `chips/lmk05318/lmk05318.rs` does the same for its front ends with its own error type;
  `PORTING.md` records the duplication.
- `src/portable/`: core only, no allocation. `board.rs` holds the wiring and every setpoint
  (`pe_sync_configure`'s frequencies and configurations); `fpga.rs` the GPO banks and bits;
  `power.rs` the power-on/off with libusdr's `float` LED fades; `status.rs` the status
  check; one module per PE_SYNC-only chip (`lmx2820/`, `lmx1214/`, `lmx1204/`:
  `reg.rs` typed registers, `plan.rs` the vendor solver, `sequence.rs` the chip
  operations; `lmk1d1208i.rs`), on shared helpers (`ti_spi.rs` the TI 24-bit SPI word and
  read-modify-write, `script.rs` fixed action lists, `chain.rs` sequencing, `poll.rs` lock
  waits, `float.rs` C's conversions and exact integer forms of `ceil`/`floor`/`log2`).
  Those chips are only PE_SYNC's in libusdr, so they stay here until a second driver
  needs them (`../../AGENTS.md`).
- `src/lowlevel.rs`: `hal`'s `Bus` seam, re-exported. `src/error.rs`: `Error`.
- `tests/common/sim_bus.rs`: `Bus` over `pe-sync-sim` (both I2C cores);
  `oracle/tests/open_parity.rs` includes it with `#[path]`, and
  `oracle/tests/chip_parity.rs` includes `src/portable` itself.

Rules: every ported sequence gets an op-for-op parity test in `oracle/tests/` against
libusdr on fresh boards; divergences are whitelisted per scenario there and listed in
`PORTING.md`. Lock waits count their poll sleeps where libusdr reads its wall clock.

## Layout decision (2026-10-12)

One crate set per device, under the `sync` kind because PE_SYNC is not an SDR. There is
**one vendored libusdr tree and one PIN**: `oracle/build.rs` compiles
`devices/vendor/libusdr` by relative path (it already lists `pe_sync.c` and its
chip drivers). Never copy it here or edit it; a PIN bump there moves this oracle too and
must re-bless the golden below in the same change. The shim copies the XSDR oracle's
pattern; a shared harness is extracted later, once the oracles are stable.

## How libusdr selects and opens the board

- **Device**: the transport hands `usdr_device_create` (`device/device.c`) a 16-byte
  device ID; `PE_SYNC_DEVICE_ID` (`device/device_ids.h`,
  `058fa4a8-752a-4115-9c09-23068dc68cdf`) selects the `pe_sync` factory. USB derives it
  from VID 0x3727 / PID 0xE001 (`lowlevel/libusb_vidpid_map.h`, SDR type `SDR_NONE`);
  the shim hardcodes it.
- **No variants**: nothing reads a hardware ID. The factory installs one constant
  parameter set (`s_params_pe_sync_rev000`: 8 SPI cores, 2 I2C cores, 0 channels) and one
  handler (`/dm/rate/master`, always `-EINVAL`).
- **Open** (`usdr_device_pe_sync_initialize`): `pe_sync_power_on` (five supply GPO banks,
  then three LED fades of 100 steps each, which double as LDO settling time);
  `pe_sync_check_status` (status word, LP87524 OTP byte, a readback from each SPI clock chip
  and both LMK1D1208Is, logged only); `pe_sync_configure`: LMK05318B (identity check,
  register map, soft reset, APLL1/APLL2 calibration and lock waits, output sync), a 2 s
  wait, LMX2820 #0 and #1 (reset, temperature sensor, tune, lock wait), LMX1214 (solver,
  divider reset), LMX1204 (reset, 4x multiplier, lock wait), both LMK1D1208Is (identity,
  outputs). Any failure powers the board off before returning.
- **Close** (`usdr_device_pe_sync_destroy`): power and LED GPOs to 0, then a 1 s LED fade.

## Operation classes and environment

Open and close use only `USDR_LSOP_HWREG` (GPO at register 0; `IGPI_STAT` at 20),
`USDR_LSOP_SPI` (24-bit words on buses 2-5) and `USDR_LSOP_I2C_DEV` (core 0 bus 0: LMK05318B
at 0x64; core 0 bus 1: LMK1D1208I at 0x68 and 0x69; core 1 bus 0: LP87524 at 0x60). No
DRP, URAM, GPI/GPO-class or custom operation occurs. SPI buses 0, 1, 6 (AD5662 DACs) and 7
(external) are not touched. The oracle panics on any of these (the C sees `-EIO`; the
panic resumes in Rust), because libusdr ignores the result of several status reads and a
quiet refusal would leave no sign in the trace.

The oracle panics if `USDR_BARE_DEV` (skips power-on and configuration) or `USDR_DEBUG`
(debug socket service) is set (`pe_sync_oracle::FORBIDDEN_ENV`). `USDR_LOGLEVEL` only
changes logging. No other variable is read on these paths.

## How far init gets

All of it, on the sim: every lock wait ends on its first poll and open returns 0. The
DPLL stays disabled (`dpll.enabled = false` in `pe_sync.c`), so the 240 s reference
validation wait is skipped; APLL2 is used (its lock is polled). The C's lock waits
(`lmk05318_wait_apll{1,2}_lock`, `lmx2820_wait_pll_lock`, `lmx1204_wait_pll_lock`) time out
by wall clock (`clock_get_time`), not by `usleep`, so an unlocked model would make the
trace depend on host speed; the sim therefore reports lock from power-on.

## What the sim models, and its assumptions

Each is stated again, with its evidence, where it is encoded.

- FPGA registers: plain storage, unwritten reads 0. GPO writes latch `data & 0xff` per
  bank (`dev_gpo_set`). `IGPI_STAT` reads 0 (only logged; its meaning is not in source).
- Chips answer regardless of the power GPOs; supplies are not tied to buses.
- LMX2820, LMX1204, LMX1214: 7-bit address, 16-bit registers, bit 23 = read. **Reset
  values and read-only masks from the vendored YAMLs** (`dflt`, `mode: R`, which also
  covers reserved bits); undocumented addresses reset to 0. Writes leave read-only bits
  alone; reset bits have no side effects; a write reads back 0. Lock detect reads locked
  (LMX2820 `R74` `RB_LD` = 2, LMX1204 `R75` `RB_LD` = 2); temperatures keep their YAML
  defaults (logged only).
- LMK05318B: 16-bit pointer, byte registers. Its YAML has no reset values, so all reset to
  0 except identity bytes 0x0000-0x0003 = 10 0b 35 42 (what `lmk05318_create` requires)
  and `PLL1/PLL2_VM_INSIDE` (0x165, 0x16f bit 5); the sim makes those read-only (the YAML
  gives no access modes). `BAW_LOCK` (0x50 bit 7, "Lock Detect Status" in the YAML) is
  stored, not reported: the C writes 1 there itself on its no-DPLL path and polls it back.
  Interrupt flags are plain storage, so cleared flags report no loss. No reset/sync side
  effects.
- LMK1D1208I: 8-bit pointer; YAML defaults (`R5` = 0x20: `REV_ID` 2, `DEV_ID` 0, accepted).
- LP87524: 8-bit pointer; only `BUCK1_CTRL1` is documented; OTP byte 0x01 reads 0.
- Absent I2C devices read `0xff` per byte (`SimBoard::without`), as in `usdr-sim`.
- I2C readback packing (first byte most significant, little-endian copy) and the
  three-out/four-in limits are transport facts from the uSDR oracle; here the LMK05318B
  identity burst confirms the packing.
- Sleeps advance virtual time only (the shim replaces `usleep`): a session is 4.27 s of
  virtual time.

## Characterization golden

`oracle/tests/golden/session.trace`: `usdr_dmd_create_string("")` then `usdr_dmd_close`,
one op per line in `pe_sync_sim::Op`'s `Display` format, `== open` / `== close` phase
markers, and a provenance header (PIN, date). It is **characterization**, not
specification. 1894 ops:

- open 1487: 608 GPO writes and the status read (power-on and LED fades are 1208 of the
  open's ops), 620 sleeps, and per chip, status reads included: LMK05318B 103 I2C,
  LMX2820 37 SPI each, LMX1204 47, LMX1214 23, LMK1D1208I 5 I2C each, LP87524 1.
- close 407: six GPO writes, then a 100-step fade.

These counts are as captured; the fixture is authoritative, so recount them on a re-bless.

`oracle/tests/identify.rs` covers selection without channels or rate, and the refusal
(`-ENODEV`) and power-off when the LMK05318B does not answer; `oracle/tests/power.rs`
asserts supplies-before-chips and the closed state. The driver is compared with live
libusdr, not with this file: `oracle/tests/open_parity.rs` covers every golden line (open
and close) and the refusals; `oracle/tests/chip_parity.rs` sweeps the LMX solvers beyond
the board's setpoints. Not covered: any control after open (there is none in libusdr
besides the refused rate), sysref windowing, DAC trims, the external SPI.

## Verify

```sh
cargo fmt -p pe-sync -p pe-sync-sim -p lmk05318 --check
cargo clippy -p pe-sync -p pe-sync-sim -p lmk05318 --all-targets -- -D warnings
cargo test -p pe-sync -p pe-sync-sim -p lmk05318
cargo check -p lmk05318 --target thumbv7em-none-eabi
python3 scripts/ci/portable.py pe-sync
cargo fmt --manifest-path devices/sync/pe_sync/nostd-check/Cargo.toml -- --check
cargo clippy --manifest-path devices/sync/pe_sync/nostd-check/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path devices/sync/pe_sync/nostd-check/Cargo.toml --locked
cargo check --manifest-path devices/sync/pe_sync/nostd-check/Cargo.toml --locked --target thumbv7em-none-eabi
cargo fmt --manifest-path devices/sync/pe_sync/oracle/Cargo.toml -- --check
cargo clippy --manifest-path devices/sync/pe_sync/oracle/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path devices/sync/pe_sync/oracle/Cargo.toml
# Re-bless the golden (PIN bump or deliberate sim change only), then review the diff:
RSL_PE_SYNC_BLESS=1 cargo test --manifest-path devices/sync/pe_sync/oracle/Cargo.toml --test init_golden
```

The oracle needs a C compiler only. libusdr is process-global, so oracles are serialised
by a lock: one is open at a time per process. CI runs it as the `pe-sync-oracle` profile;
a vendored libusdr change, `pe-sync` or `lmk05318` selects it too. The fixture runs
as `pe-sync-portable`.

## Open items and blockers

- Lock is reported, not acquired (and LMK05318B `BAW_LOCK` is whatever the C wrote):
  modelling calibration (`FCAL_EN`, `PLLn` calibrate triggers) is needed before testing a
  driver's unlocked or retry paths, which in libusdr also need a fake wall clock
  (`clock_get_time` is not redirected).
- LMK05318B reset values and its DPLL (`REFVALSTAT`, reference validation) are not in the
  PIN subset; enabling the DPLL needs both.
- The YAML reset tables were transcribed from the PIN by script; nothing re-derives them
  on a PIN bump, so the bump must re-check them.
- The AD5662 DACs, sysref windowing (`lmx1204/lmx1214_sysref_windowing_*`) and the
  external SPI are not modelled; nothing in open/close uses them.
