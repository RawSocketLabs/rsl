# pe-sync porting ledger

The PE_SYNC row of the programme ledger (`../../sdr/usdr/PORTING.md`, "PE_SYNC clock tree /
distribution / synchronization") in detail. Same contract: **Present** means source
exists, test names are definitions, not run results; nothing is hardware-qualified (U).
Reference: `devices/vendor/libusdr/PIN`; `C:` paths are relative to its `lib/`.
`O:` = `oracle/tests/`, `T:` = `tests/`, `P:` = `src/portable/`.

Golden-line numbers refer to `oracle/tests/golden/session.trace` (characterization).

## Done

| Capability | Pinned C | Rust owner | Golden lines | Evidence |
|---|---|---|---|---|
| Power-on: five supply GPO banks, three LED fades (libusdr's `float` brightness steps kept) | `C:device/pe_sync/pe_sync.c::pe_sync_power_on`, `leds_fader` | `P:power.rs` (`Power::on`), `P:fpga.rs` | 9–1216 | `O:open_parity.rs::a_pe_sync_session_matches_libusdr_operation_for_operation`; `P:power::tests` / U |
| Status check: status word, LP87524 OTP byte, LMX readbacks with MUXOUT readback, LMK1D1208I identities | `pe_sync.c::pe_sync_check_status` | `P:status.rs` | 1217–1226 | same; `O:open_parity.rs::a_silent_pmic_changes_nothing_but_the_byte_both_log`; `P:status::tests` / U |
| LMK05318B: identity, XO/APLL1/output plan, staged registers, soft reset, APLL1/APLL2 calibration and lock waits, lock-status read, output sync | `C:hw/lmk05318/lmk05318.c::lmk05318_create`, `_wait_apll1_lock`, `_wait_apll2_lock`, `_check_lock`, `_sync` | `src/lmk05318.rs` on `lmk05318` (`plan`, `staging`, `map`, value types); setpoints `P:board.rs` | 1227–1339 | same; `O:open_parity.rs::a_board_without_its_lmk05318_is_refused_and_powered_off_like_libusdr`; usdr's `O:lmk05318_parity.rs` for the plan / U |
| LMX2820 ×2: reset, temperature sensor, status; solver (reference doubler/multiplier/dividers, VCO core, fraction, channel dividers and muxes); tune registers; lock wait | `C:hw/lmx2820/lmx2820.c::lmx2820_create`, `lmx2820_solver`, `lmx2820_tune`, `lmx2820_wait_pll_lock`, `lmx2820_read_status` | `P:lmx2820/` (`Create`, `Plan`, `Tune`, `status`) | 1340–1417 | same; `O:chip_parity.rs::lmx2820_tunes_match_libusdr_across_references_orders_and_outputs`; `P:lmx2820::{plan,sequence}::tests` / U |
| LMX1214: setup and temperature; solver (buffer/divider, auxiliary pre-divider/divider/bypass); divider reset around the registers | `C:hw/lmx1214/lmx1214.c::lmx1214_create`, `lmx1214_solver`, `lmx1214_get_temperature` | `P:lmx1214/` (`create`, `Plan`, `configure`, `temperature`) | 1418–1440 | same; `O:chip_parity.rs::lmx1214_solutions_match_libusdr_across_modes_and_auxiliary_paths`; `P:lmx1214::*::tests` / U |
| LMX1204: setup, temperature and status; solver (buffer/divider/multiplier, state machine clock, logic clock, SYSREF delay/pre-divider/divider in all three modes); soft reset and registers; multiplier lock wait and state machine clock switch-off | `C:hw/lmx1204/lmx1204.c::lmx1204_create`, `lmx1204_solver`, `lmx1204_read_status`, `lmx1204_wait_pll_lock` | `P:lmx1204/` (`create`, `Plan`, `configure`, `status`, `WaitLock`) | 1441–1487 | same; `O:chip_parity.rs::lmx1204_solutions_and_lock_waits_match_libusdr_across_modes`; `P:lmx1204::*::tests` / U |
| LMK1D1208I ×2: identity check, output enables, swings, inputs and banks | `C:hw/lmk1d1208i/lmk1d1208i.c::lmk1d1208i_create` | `P:lmk1d1208i.rs` (`Create`) | 1488–1495 | same; `O:open_parity.rs::a_board_without_a_fan_out_buffer_is_refused_and_powered_off_like_libusdr`; `P:lmk1d1208i::tests` / U |
| Power-off on a failed open; close | `pe_sync.c::usdr_device_pe_sync_initialize`, `pe_sync_power_off`, `usdr_device_pe_sync_destroy` | `Device::open`, `Device::close`, `P:power.rs` (`Power::off`) | 1496–1903 | `O:open_parity.rs` (all four tests); `T:open.rs` / U |

Each register sequence is a hand-written sans-I/O machine over `hal`'s `Sequence`,
run by its blocking `run_sequence`; `nostd-check/` compiles the exact `src/portable` tree for
`thumbv7em-none-eabi` and runs its machines. The LMK05318's bus traffic is blocking code
(see below).

## Divergences (whitelisted per scenario)

None in the tested scenarios: every compared trace matches libusdr exactly. Behaviour that
differs where the oracle cannot run it:

| Divergence | Why | Where tested |
|---|---|---|
| Lock waits count their poll sleeps (LMK05318 100 µs × 1000, LMX2820 100 µs × 10 000, LMX1204 1 ms × 1000) where libusdr reads the wall clock | Deterministic on a simulator; waits at least as long on hardware | `P:poll::tests`, `P:lmx2820::sequence::tests::an_unlocked_pll_times_out_after_a_second_of_polls`, `P:lmx1204::sequence::tests::an_unlocked_multiplier_times_out_after_a_second_and_keeps_its_clock` |
| An LMX2820 output pair whose ratio is not an exact power of two is refused at once; libusdr refuses it later in the same solver (its 1e-8 `log2` tolerance admits near ratios its exactness checks then reject) | Same refusal, no bus access in either | `O:chip_parity.rs` (outcomes compared) |
| An LMX1204 SYSREF output of 0 Hz in generator modes is refused; libusdr divides by zero | Undefined in C | Not reachable from `PE_SYNC`'s setpoints; `Plan::solve` |

Kept as libusdr, not divergences: the LMK1D1208I identity check refuses only a chip wrong in
both revision and device (`rev_id != 2 && dev_id != 0`); an LED fade's per-period writes are
not checked, only its last; status readbacks are read and discarded (libusdr logs them); the
LMX1204 solver's search starts with `found = true` and carries on past its last pre-divider;
LMX1204's lock-time state machine write ignores its result; the LMX1204's SYSREF request
input is LMX2820 #0's planned 25 MHz though that synthesizer's SYSREF stays off.

## Remaining

| Capability | Pinned C | Blocker or note |
|---|---|---|
| A transport (USB VID 0x3727 PID 0xE001, PCIe) | `C:lowlevel/usb_uram/`, `pcie_uram/` | PE_SYNC has two I2C cores; `usdr`'s transports refuse core ≠ 0 and know the uSDR's register layout. Shared transports belong outside `usdr` first |
| LMK05318 bus code shared with `usdr` | `C:hw/lmk05318/lmk05318.c` | Duplicated today (`src/lmk05318.rs` here, `chips/lmk05318/lmk05318.rs` in `usdr`; the plan and registers are shared). Merge into `lmk05318` once `hal`'s seam carries 16-bit-pointer and multi-byte I2C transfers, or with an error mapping both drivers accept |
| LMK05318 DPLL (reference validation, `REFVALSTAT`) | `lmk05318_dpll_config`, `lmk05318_wait_dpll_ref_stat` | `pe_sync.c` leaves it off; sim lacks reset values and reference validation |
| LMX2820 SYSREF and instant calibration; LMX1204/LMX1214 precision mode, per-output SYSREF delays, SYSREF windowing; LMX2820 `sync` | `lmx2820_calculate_systef_chain`, `lmx2820_instant_calibration_init`, `*_sysref_windowing_*`, `lmx1204_init_sysrefout_ch_delay` | Not called by `pe_sync.c`; no control in libusdr reaches them |
| AD5662 DACs, external SPI | `pe_sync.c` (SPI buses 0, 1, 6, 7) | Not driven at open; not modelled |
| Unlocked and retry paths against libusdr | — | Sim reports lock at once; libusdr's waits need a fake wall clock (`AGENTS.md`, open items) |
| One home for C's numeric helpers | — | `gcd`, `abs` and the C conversions (`c_f64`, `c_u64`, `c_unsigned`) exist three times: `P:float.rs`, `lmk05318`'s `plan.rs` and `usdr`'s `cal::search::gcd`. Consolidate once a shared `no_std` home is agreed |
| Hardware qualification | — | Not authorized |
