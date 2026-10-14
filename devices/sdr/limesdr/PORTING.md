# limesdr porting ledger

The LimeSDR family (`u3_limesdr`) rows of the programme ledger (`../usdr/PORTING.md`,
"LimeSDR board / framing / streams" and "FT601 USB transport") in detail. Same contract:
**Present** means source exists, test names are definitions, not run results; nothing is
hardware-qualified (U). Reference: `../../vendor/libusdr/PIN`; `C:` paths are relative to
its `lib/`. `O:` = `oracle/tests/`, `T:` = `tests/`, `P:` = `src/portable/`, `L:` =
`../chips/lms7002m/src/` (`lms7002m`).

Golden-line numbers refer to `oracle/tests/golden/limesdr_mini_session.trace`
(characterization): lines 10–71 are the open, 72 and 73 the empty power-up and close.

## Done

| Capability | Pinned C | Rust owner | Golden lines | Evidence |
|---|---|---|---|---|
| LMS64C framing: single-frame register writes and reads for the gateware and the LMS7002M, the LMS7002M reset pulse, reply values | `C:ipblks/lms64c_proto.c::lms64c_fill_packet`, `lms64c_parse_packet`; `lms64c_cmds.h`, `lms64c_proto.h` | `P:lms64c.rs` (public `limesdr::lms64c`) | every `req`/`rep` line | `O:lms64c_parity.rs` (0 to 15 records, both register files, reset, 14 reply values, against the compiled C); `P:lms64c::tests` / U |
| `ls_op` → LMS64C mapping (gateware 16-bit write/read, the 32-bit count read over two registers, LMS7002M SPI by write flag, custom command 0 as the reset pulse; a status other than completed, or a reply to another command, fails) | Oracle shim `oracle/shim/sim_plugin.c` (assumption: the FT601 transport is not vendored) | `P:lsop.rs` (`LsOp`), `src/device.rs` (one exchange per operation) | every `req`/`rep` line | `O:open_parity.rs` (all scenarios); `P:lsop::tests`; `T:open.rs::a_failed_exchange_names_the_register_it_struck` / U |
| Open: reference-clock measurement (start, polls 50 ms apart, count), LMS7002M reset pulse, the exact 0x3841 version gate, `lms7002m_create` (typed reset programme and version check), stream-control stop | `C:device/u3_limesdr/limesdr_ctrl.c::limesdr_init`, `limesdr_detect_refclk`, `limesdr_stop_streaming`; `C:hw/lms7002m/lms7002m.c::lms7002m_create` | `P:open.rs` (`Open`), `P:reference.rs`, `P:fpga.rs`, `L:create.rs` (`Create`), `L:spi.rs` (`spi_read`), `Device::open` | 10–71 | `O:open_parity.rs::a_fresh_session_matches_libusdr`, `a_detector_taking_its_gate_time_is_polled_and_waited_for_like_libusdr`, `an_lms7002m_other_than_mask_1_is_refused_like_libusdr`, `references_libusdr_decides_reliably_are_decided_alike`; `T:open.rs`; `P:open::tests`, `P:reference::tests` / U |
| Power-up (`/dm/power/en`) and close: no bus operation | `C:device/u3_limesdr/u3_limesdr.c::dev_limesdr_pwren_set`, `limesdr_device_destroy` → `limesdr_ctrl.c::limesdr_dtor` ("TODO") | `Device::power_up`, `Device::close` | 72–73 | `O:open_parity.rs::a_fresh_session_matches_libusdr`; `T:open.rs::power_up_and_close_touch_nothing` / U |
| Identity: the emulated `/dm/revision` | `u3_limesdr.c::dev_limesdr_sdr_revision_get` | `Device::revision` | — | `O:open_parity.rs::the_revision_is_libusdrs_emulated_constant` / U |
| `no_std` compile witness for `src/portable` | — | `nostd-check/` (`limesdr-nostd-check`, exact-source `#[path]` import), `scripts/ci/portable.py limesdr` | the open's 20 exchanges | `nostd-check` tests; `cargo check --target thumbv7em-none-eabi` (release-only in CI) / U |

The open is a hand-written sans-I/O machine (`Open`: `next` names a `LsOp` or a wait,
`complete` takes its result) run by `Device::open`; `lms7002m`'s `Create` runs inside it
on the same seam. The portable tree uses core only and allocates nothing (held by review;
`bnb`, under `lms7002m`'s value types, requires `alloc`). The created chip's cached state
is not kept yet: nothing after the open reads it.

## Divergences (whitelisted per scenario)

| Divergence | Why | Where whitelisted |
|---|---|---|
| The reference is the nominal one (10, 30.72, 38.4, 40, 52 MHz) nearest the measurement within 1 MHz, the higher of two equally near. libusdr's walk skips entries and wraps: it opens only [30.72, 31.72] MHz (as 30.72) and [40, 41] MHz (as 40); it refuses exactly 10 MHz, (31.72, 38.4] and (41, 52) MHz with `-EFAULT`; it reads past its table at `clkTbl[5]` for (10, 30.72), (38.4, 40) and [52, 110] MHz, and at `clkTbl[i - 1]` with unsigned `i` = 0 below 10 or above 110 MHz (`P:reference.rs` has the table) | A real 40 MHz oscillator a few ppm slow lands in libusdr's undefined case; the fix is the walk with `ABS` evaluated once and signed | `O:open_parity.rs::references_libusdrs_walk_misses_are_recognised_and_opened_as_from_40_mhz` (10, 38.4, 52 MHz): libusdr's operations up to its refusal match, then limesdr's match a 40 MHz open from the reset on, with the reference recognised as itself; `references_libusdr_decides_reliably_are_decided_alike` covers 31, 35, 40, 40.9 and 41.1 MHz, where both decide alike. The undefined cases cannot run in libusdr: `T:open.rs::a_40_mhz_reference_a_few_ppm_slow_opens` |
| A detector that never finishes: `Error::DetectorStalled` after libusdr's ten polls and waits; libusdr tests `j == 100` after at most ten polls, so it reads whatever count the registers hold and may open on a stale one | A stale count is not a measurement | `O:open_parity.rs::a_stalled_detector_is_refused_where_libusdr_opens_on_a_stale_count` (the same polls and waits, then libusdr's count read) |
| The measurement is kept 64 bits wide; libusdr's 32-bit estimate wraps for counts above about 720.6 million (4.29 GHz), which could alias a nominal reference | No board produces such counts | Not observable on the sim's references; `P:reference::tests::counts_no_board_produces_do_not_wrap_onto_a_reference` |
| A reply naming another command than its request fails the exchange (`ExchangeError::Mismatched`); libusdr's shim checks only the status | A reply out of step with its request is not its result | Not reachable on the sim, which echoes the command: `P:lsop::tests::a_reply_to_another_command_is_not_this_operations_result` |
| A failed open reports its own error; libusdr's API reports `-ENODEV` for any device init failure (`lowlevel_create` returns the last transport plugin's result) | The device's errno is what failed | Outcomes, not operations: `O:open_parity.rs` compares the device's errno (`OpenError::device_errno`) where it checks one |

Kept as libusdr, not divergences: the reset pulse precedes any LMS7002M access; the version
read happens twice (the board's exact gate, then `lms7002m_create`'s VER/REV check); stream
control is read and written back with only `RX_EN` and `TX_EN` cleared; close leaves the
LMS7002M, its reset pin and the gateware as the open left them.

## Remaining

| Capability | Pinned C | Blocker or note |
|---|---|---|
| FT601 USB transport (a `ControlPort` over `usb`): endpoint setup, framing of LMS64C frames on the bulk pipes, transport-level open traffic (board info, FIFO reset), USB identity by VID:PID (`0x0403:0x601f`, `0x0403:0x601e`) | `C:lowlevel/usb_ft601/usb_ft601_{generic,libusb}.c` (upstream, **not vendored**); `lowlevel/libusb_vidpid_map.h` | Not provable without hardware or the C: vendor it (a PIN-subset change in `../../vendor/vendor.sh`), then frame-test against it. It would also turn the `ls_op` → LMS64C mapping from an assumption into evidence |
| Firmware reply semantics (echoed header, record layout, write-flag handling, other reset levels) | Not in libusdr; LimeSuite or the firmware | The sim and the driver share the shim's reading |
| Close on drop | `usdr_dmd_close` | Nothing to do today: libusdr's close issues nothing |
| Sample rate, tuning, gain, bandwidth, paths, antenna switch (`_limesdr_antenna_port_switch`, register 0x0017), direct clocking (0x0005) | `limesdr_ctrl.c::limesdr_set_samplerate`, `limesdr_set_direct_clocking`; `u3_limesdr.c` parameter handlers; `C:device/m2_lm7_1/lms7002m_ctrl.c` | The LMS7002M rate/tune/gain operations land in `lms7002m` (in progress for the XSDR); need the LMS7002M reset values and channel banking in `lms7002m-sim`; the driver must then keep `Create`'s chip state |
| Streams: `limesdr_setup_stream`, `limesdr_prepare_streaming` (`lms7002m_streaming_up`), `limesdr_disable_stream`, the timestamp reset, the 16-byte data header | `limesdr_ctrl.c`; `C:ipblks/streams/stream_limesdr.c` | Needs the FT601 transport's stream pipes; `lms7002m_streaming_up` belongs in `lms7002m` (`../../AGENTS.md`) |
| Multi-frame LMS64C requests (more than 14 records) | `lms64c_fill_packet` with `out_cnt` > 1 | Nothing in the open needs them; libusdr's parser misreads reads over 14 registers (`AGENTS.md`) |
| Hardware qualification | — | Not authorized |
