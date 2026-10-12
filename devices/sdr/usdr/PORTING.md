# Full libusdr port: capability ledger and roadmap

Approved scope, recorded 2026-10-12: full **functional** libusdr replacement, uSDR first,
then every registered family. Rust API only; Linux first. The program is **not complete**.
The first private execution pilot is **Present**; the full nonblocking driver is
**In progress** and physical operation is **Unqualified**.
The portable-core-first amendment is integrated: selected production algorithms are shared
unchanged with an unpublished no_std + alloc compile witness. The public driver still uses
std; no embedded or kernel deployment is implied.
All crates remain unpublished. This document authorizes neither hardware operations nor
release/publication, kernel/FPGA rewrites, or a production C fallback.

## Reference and status contract

Baseline: `oracle/libusdr/PIN`, **3670f15de8e49691db455da03166c17f6dd5770a**.
`C:` paths below are relative to upstream `src/lib/` at that commit; most are retained
under `oracle/libusdr/lib/`. Excluded transport implementations and tools are in the full
upstream checkout, not silently assumed to exist in the vendored subset. `tools:` paths
are relative to upstream `src/tools/`. The reference is available at
[the pinned source tree](https://github.com/wavelet-lab/usdr-lib/tree/3670f15de8e49691db455da03166c17f6dd5770a/src).
Never hand-edit the vendor subset; source refresh and PIN changes are separate changes.

Each row records implementation / software evidence / hardware status separately.
**Present** means source exists, not that a check ran. **Partial** means only the stated
subset exists. **Missing** has no implementation. Software names are existing test
definitions, **not run results**. **None** means no relevant current test was located.
Hardware **U** means Unqualified; no current row is hardware-qualified. Planned owners
are destinations, not declarations of existing public API. Unsupported reference stubs
are distinguished from genuine capability. Split a row before claiming only part of it done.

Test prefixes: `O:` = `oracle/tests/`, `T:` = `tests/`, `UT:` = `src/transport/usb/tests.rs`,
`I2C:` = `src/transport/usb/i2c_tests.rs`, `EX:` = `src/transport/usb/execution_tests.rs`,
`NAT:` = `src/transport/usb/native/tests.rs`.
`USB:` = workspace `usb/src/queue.rs::tests` (generic host-queue boundary).
`N:` = `nostd-check/src/lib.rs`; `P:` = `src/portable/`.
`uSDR` means m2_lm6_1 revisions 1–3; `USB/PCIe` means the Linux host implementations.
Board-parity tests use a simulated bus and do not qualify either physical transport.

## Retained uSDR baseline

| Capability | Pinned C evidence | Rust owner/interface | Variant / path | Implementation / software evidence / hardware | Divergence or limit |
|---|---|---|---|---|---|
| Identify board and power up/down | `C:device/m2_lm6_1/usdr_ctrl.c`, `usdr_ctor`, `usdr_init`, `usdr_dtor` | `Board::identify`, `board/power.rs` (rails via `portable/power.rs`); `Device::close` | uSDR; simulated bus, USB/PCIe | Present / `O:power_parity.rs::rev1_power_cycle_matches_libusdr`, rev2/rev3 equivalents / U | Front-end probe omitted; unknown revisions rejected. |
| Temperature conversion | same C file, `usdr_gettemp` | `Device::temperature`, TMP114 | uSDR revisions with sensor | Present / `O:temperature_parity.rs::temperatures_match_libusdr_across_the_sensor_range` / U | Additional thermal policy gates power-up; 110 °C hard stop cannot be disabled. |
| Thermal waiting and refusal | C initialization temperature checks; Rust policy is an addition | `thermal.rs`, `DeviceBuilder`, `Device::wait_for_temperature` | uSDR | Present / `T:thermal.rs::the_wait_never_overshoots_its_timeout`, `hard_stop_only_refuses_at_exactly_the_hard_stop` / U | Policy intentionally exceeds C behaviour; no blind copy of ignore-temperature environment override. |
| RX sample rate and decimation | `C:device/m2_lm6_1/usdr_ctrl.c`, `usdr_set_samplerate_ex` | `Device::set_rx_sample_rate`, `board/rate.rs` | uSDR RX | Present / `O:rate_parity.rs::rev3_rx_only_rate_changes_match_libusdr`, rev1/rev2 rate tests / U | Not independent TX/ADC/DAC rate control. |
| RX RF tuning, mixer-band crossing and low-LO fallback | same C file, `usdr_rfic_fe_set_freq` | `Device::set_rx_frequency`, `board/tune.rs` | uSDR RX | Present / `O:rx_parity.rs::the_low_lo_fallback_matches_libusdr`, `the_nco_headroom_limit_matches_libusdr`; `T:band_crossing.rs::crossing_the_mixer_band_only_flips_output_3` / U | Band crossing gates OUT3 only rather than cycling the clock state. |
| Fixed RX bandwidth / filter bypass | same C file, `usdr_rfic_bb_set_badwidth` | `Device::set_rx_bandwidth` | uSDR RX | Present / `O:rx_parity.rs::fixed_bandwidth_holds_across_rate_changes_like_libusdr`, `wide_bandwidth_bypasses_the_filter_like_libusdr` / U | Preserve existing rate/frequency-following rules. |
| Startup DC and LPF calibration | same C file, `usdr_calib_dc` | `chips/lms6002d/lms6002d.rs::calibrate_rx_dc` | uSDR RX startup | Present / `O:stream_parity.rs::a_tuned_stream_matches_libusdr` / U | RX calibration also touches TX PLL/LPF; that does not provide TX streaming. |
| CI16 channel-zero stream creation/start/receive/stop | `C:device/m2_lm6_1/m2_lm6_1.c`, `usdr_device_m2_lm6_1_create_stream`; `C:ipblks/streams/stream_sfetrx4_dma32.c` | `Device::{start_rx_stream,receive,stop_rx_stream}`, `board/stream.rs` | uSDR RX; simulated bus, USB/PCIe | Present / `O:stream_parity.rs::received_samples_match_libusdr`, `a_two_burst_stream_on_rev1_matches_libusdr` / U | Omits C's erroneous per-channel baseband retune loop. |
| RX loss/timestamp accumulation and burst padding | `C:ipblks/streams/stream_sfetrx4_dma32.c`, `_sfetrx4_stream_recv` | `RxPacket`, `board/stream.rs` | uSDR CI16 | Present / `T:stream.rs::lost_packets_advance_the_sample_count_and_accumulate`, `a_padded_packet_arrives_without_its_stubs` / U | Simulator patterns are not RF; actual transport losses unqualified. |
| USB register framing/indexed window access | `C:lowlevel/usb_uram/usb_uram_libusb.c`, `usb_async_regwrite32`, `usb_async_regread32`; `usb_uram_generic.c::usb_uram_reg_op` | `UsbBus`, `transport/window.rs` | uSDR; Linux USB | Present / `UT:raw_register_frames_and_chunk_limits_match_c`, `indexed_registers_are_selected_for_every_word` / U | Exact lengths, bounded addressing; malformed/ambiguous transactions fail closed. |
| USB SPI / I²C LUT commands and completion routing | `C:lowlevel/usb_uram/usb_uram_generic.c::usb_uram_ls_op`; `C:ipblks/si2c.c` | `UsbBus`, `transport/usb/protocol.rs` | uSDR; Linux USB | Present / `O:usb_i2c_parity.rs::cache_and_control_words_match_c_for_replacement_hits_and_all_lengths`; `UT:spi_and_i2c_use_separate_irq_completions` / U | SPI always waits; I²C read waits; write-only I²C does not. |
| USB interrupt initialization / control failure handling | `C:lowlevel/usb_uram/usb_uram_generic.c::usb_uram_generic_create_and_init`; libusb notification callback | `UsbBus` | uSDR; Linux USB | Present / `UT:initialization_matches_pinned_router_sequence`, `timeout_never_assigns_delayed_reply_to_a_new_request` / U | Refuses PCIe ownership; stricter than C sequence resynchronization/setup skip. |
| USB RX payload/trailer, ring reuse and timeout/close | `C:lowlevel/usb_uram/usb_uram_libusb.c::_usb_uram_init_rxstream`, `usb_uram_recv_dma_wait`, `usb_uram_recv_dma_release` | `transport/usb/rx.rs` over `rsl_usb::InQueue` | uSDR; Linux USB endpoint 0x83 | Present / `O:usb_rx_parity.rs::raw_samples_and_all_trailer_words_match_the_pinned_c_receiver`, `timeout_lends_no_buffer_and_both_receivers_can_close_without_release` / U | Bounded owned ring rather than discard/autorestart; malformed blocks fail closed; timeout preserves ownership; cancellation failure requires close/drop and reopen at Device level. |
| PCIe userspace ABI / register and RX access | `C:lowlevel/pcie_uram/pcie_uram_main.c`; `pcie_uram_driver_if.h` | `transport/pcie.rs`, `rsl-usdr-pcie` | uSDR; Linux kernel ABI3 | Present / `O:pcie_abi.rs::sizes_offsets_and_request_codes_match_the_header`, `the_usdr_layout_matches_libusdrs` / U | ABI checks are not DMA, IRQ or driver lifecycle qualification. |

## Remaining uSDR controls, TX and execution

| Capability | Pinned C evidence | Rust owner/interface | Variant / path | Implementation / software / hardware | Blocker or boundary |
|---|---|---|---|---|---|
| Sans-I/O operation/continuation execution | `C:lowlevel/usdr_lowlevel.h` operations plus board sequences | `TemperatureRead`, `I2cTransaction`, `PowerRails` (PMIC/boost part of power-up), Si5332 `ClockSequence` (program, state wait, sample-clock layout) over the `Sequence` seam; the blocking temperature/I²C, power-rail and clock paths run them | uSDR first; private portable core | Partial temperature/I²C/power rails / `I2C:tmp114_request_and_scalar_reply_match_pinned_usb_golden`, `unrelated_irqs_do_not_restart_completion_deadline`, `P:power::tests::a_wrong_pmic_revision_stops_before_any_write`, `P:si5332::sequence::tests::a_state_that_never_settles_is_accepted_after_a_hundred_polls`, `a_missing_input_clock_fails_only_the_wait_after_the_writes`, `O:rate_parity.rs` (unchanged), `N:tests::the_exact_source_witness_runs_the_pinned_clock_sequences`, `every_channel_is_controlled_once_and_the_rails_finish`, `O:power_parity.rs` (unchanged), `N:tests::the_exact_source_witness_runs_the_pinned_power_rail_sequence`, cancellation/identity/clock/failure cases / U | FPGA/GPO glue, RF release/configuration, tuning and RX still use the blocking Bus; `NativeUsb` does not yet execute `PowerRails` (it lacks register/GPO access over 0x81). No general scheduler or runtime in core. |
| Portable core: addresses, chip values/math, temperature, power rails and framed I²C | Same pinned chip/transport implementations; TMP114 SNIS214E §8.6 | Internal `portable` tree (incl. LP8758/TPS6381x values, `PowerRails`); thin native adapters; exact-source `nostd-check` | Host driver plus thumbv7em-none-eabi compilation witness | Present for listed subset / `N:tests::the_exact_source_witness_executes_the_pinned_temperature_transaction`, `P:temperature::tests`, `P:clock::tests::invalid_clock_inputs_never_reach_a_zero_divisor`, `P:power::tests`, `N:tests::the_exact_source_witness_runs_the_pinned_power_rail_sequence`; `scripts/ci/portable.py` contract and target compile gate / U | no_std + alloc compilation, not full-driver no_std, embedded linking, allocator-free dependencies or kernel suitability. Invalid zero/wrapped-zero divider inputs reject rather than panic; supported C arithmetic is unchanged. |
| Bounded native USB completion primitives | `C:lowlevel/usb_uram/usb_uram_libusb.c` async transfer callbacks; Linux usbfs `REAPURBNDELAY` | `rsl_usb::Interface::reap`, `InQueue::try_complete`, `OutQueue::try_complete`; existing owned `submit` | Linux USB; portable host API | Present / `USB:trying_a_completion_never_reaps_or_waits_for_it`, `shared_reaping_stops_at_its_budget_and_routes_both_directions`, budget/contention/prefix-error/disconnect cases / U | No driver scheduling. Kernel ioctl locks/memory faults may delay calls; bounded reaping is not a hard-real-time executor. |
| Worker-owned USB cancellation boundary | Linux usbfs `DISCARDURB` calls `usb_kill_urb`, not a nonblocking unlink | Whole `InQueue`/`OutQueue` moved to caller-owned worker; existing `cancel_all` and failure-path Drop stay there | Linux USB; fake kernel ownership tests | Partial documented handoff / `USB:worker_cancellation_retains_queue_ownership_until_discard_returns`, `dropping_a_cleanup_observer_does_not_release_the_workers_queue`, partial-discard and concurrent-reap cases / U | No shipped managed worker/executor. Worker must own buffers until return; do not join on reactor. Other ioctls can wait behind the device kernel lock, requiring isolation in the native executor. |
| Native USB readiness registration | Linux usbfs `usbdev_poll` (writable while completed URBs await reaping; HUP/ERR on disconnect) | `rsl_usb::Interface::readiness` → `Readiness: mio::event::Source` (feature `mio`); one per device, self-deregistering | Linux USB | Present / `USB:a_driven_devices_waiter_leaves_readiness_to_the_reactor`, `cancelling_on_a_driven_device_needs_no_reactor_turn`, `releasing_the_reactor_lets_its_waiters_reap_for_themselves`; `usb/tests/hardware.rs::readiness_is_exclusive_deregisters_on_drop_and_holds_off_reset` (opt-in, not run) / U | Consumed by the uSDR `NativeUsb` handle (row below). Real-node registration and event delivery are unverified. |
| External readiness / deadline reactor | C pollable stream interface; `C:models/dm_stream.c::usdr_dms_get_fd` | Unstable `transport::usb::NativeUsb` via `UsbBus::into_native` (feature `mio`), temperature only, over private `native::{Adapter, Worker, Queues}`: reactor-side registration, deadlines and coalesced scheduling; once attached, one worker owns every queue and syscall, the readiness's Drop, cleanup and Drop | Linux USB first; PCIe missing | Partial temperature-only handle / `NAT:the_native_handle_reads_the_sensor_repeatedly_and_cleans_up_on_its_worker`, `the_handle_continues_the_blocking_bus_notification_sequence`, `native_fd_and_worker_execute_the_production_temperature_transaction`, `exhausted_budget_reschedules_without_another_device_edge`, `a_stuck_submit_cannot_block_deadlines_or_release_worker_ownership`, `a_lost_registration_still_retires_the_worker`, panic/expiry/error cases; `T:usb_hardware.rs::a_real_usb_board_reports_its_temperature_through_a_caller_owned_reactor` (opt-in, not run) / U | Unit tests use a socket source and whole-transfer fake; real `Queues` (usbfs reap/`Busy`/cleanup) runs only in the unrun hardware test. Takes over the whole device and applies no thermal policy (it powers nothing); one request in flight. Power, RX, TX, fairness, timers and browser integration remain open. |
| Managed synchronous/asynchronous driving | Same functional operations, not C threading API compatibility | Planned opt-in drivers; test-only observers share the real machines | Linux first | Missing production adapters / `EX:managed_mio_runner_allows_a_blocking_observer_on_another_thread`, `future_poll_only_observes_and_completion_wakes_the_registered_observer`, observer-drop/driver-drop cases (pilot only) / U | No shipped runtime/driver API; explicit native lifecycle/worker ownership remains open. |
| Bounded RX/control composition pilot | C's independently progressing control/stream endpoints | Existing `Rx` adapter plus temperature/I²C core at whole-transfer fake boundary | Synthetic frames, Linux host tests | Partial private pilot / `EX:rx_poll_timeout_preserves_eight_leases_while_control_makes_progress`, `a_cross_thread_sample_borrow_is_not_requeued_while_control_completes`, `saturated_rx_does_not_block_abort_and_failed_cleanup_keeps_ownership` / U | Demonstrates loan/requeue/cleanup policy, not native independent stream handles, DMA timing or RF. |
| RX VGA1 gain | `C:device/m2_lm6_1/m2_lm6_1.c::dev_m2_lm6_1_sdr_rx_gainvga_set` | Planned RX gain API/chip driver | uSDR | Missing / None / U | Not the startup default register value. |
| RX VGA2 combined gain | same C file, `dev_m2_lm6_1_sdr_rx_gainpga_set` | Planned RX gain API | uSDR | Missing / None / U | Preserve reference gain scaling/range. |
| RX VGA2A / VGA2B independent gain | same C file, `dev_m2_lm6_1_sdr_rx_gainvga2a_set`, `...gainvga2b_set` | Planned RX stage controls | uSDR | Missing / None / U | Distinct stages, not aliases of combined gain. |
| RX LNA / automatic gain policy | same C file, `dev_m2_lm6_1_sdr_rx_gainlna_set`, `...gainauto_set` | Planned RX gain policy | uSDR | Missing / None / U | Automatic reference policy is not proof of closed-loop RF AGC. |
| Manual antenna/path selection | same C file, `dev_m2_lm6_1_sdr_rx_path_set` | Planned RX path controls | uSDR | Missing / None / U | Automatic band selection already exists; manual override is separate. |
| RX manual DC correction/correction mode | same C file, `...rx_dccorr_set`, `...rx_dccorrmode_set` | Planned RX correction API | uSDR | Missing / None / U | Preserve startup DC calibration rather than replacing it; measurement getter is a reference stub below. |
| RX phase/gain and IP2 correction | same C file, `...rx_phgaincorr_set`, `...rx_ip2corr_set` | Planned RX correction API | uSDR | Missing / None / U | Requires exact C arithmetic and register evidence. |
| Independent RX/TX and ADC/DAC rate plans | same C file, `dev_m2_lm6_1_rate_m_set`; `usdr_ctrl.h::USDR_SR_MAXCONVRATE` | Planned typed rate plan | uSDR RX/TX | Partial RX / None for extended plans / U | Keep actual/effective rate distinct; existing RX rate parity is retained. |
| Explicit per-channel baseband offsets / coherent NCO reset | `C:device/m2_lm6_1/usdr_ctrl.c::usdr_rfic_fe_set_freq`, `usdr_reset_txnco` | Planned baseband/NCO controls | uSDR | Partial automatic RX offsets / None for controls / U | No restoration of the stream-create retune bug. |
| TX RF tune / manual path / bandwidth | `C:device/m2_lm6_1/m2_lm6_1.c::dev_m2_lm6_1_sdr_tx_freq_set`, `...tx_path_set`, `...tx_bandwidth_set` | Planned TX board/chip API | uSDR USB/PCIe | Missing / None / U | Shared PLL value types exist; no public TX operation yet. |
| TX VGA1 / VGA2 / automatic gain | same C file, `...tx_gain_vga1_set`, `...tx_gain_vga2_set`, `...tx_gainauto_set` | Planned TX gain API | uSDR | Missing / None / U | Distinct reference controls/ranges. |
| TX DC and phase/gain correction | same C file, `...tx_dccorr_set`, `...tx_phgaincorr_set` | Planned TX correction API | uSDR | Missing / None / U | Startup RX calibration touching TX is not coverage. |
| TX constant generator | same C file, `...sdr_tfe_gen_const_set` | Planned TX generator/control API | uSDR | Missing / None / U | Typed capabilities rather than unchecked raw values; antenna timing handler is a reference mockup below. |
| TX stream create/start/send/stop and timestamps | same C file, `usdr_device_m2_lm6_1_create_stream` TX branch; `C:ipblks/streams/stream_sfetrx4_dma32.c::_sfetrx4_stream_send` | Planned TX stream / transport queues | uSDR USB/PCIe | Missing / None / U | Include old/extended TX cores, underrun and cleanup semantics. |
| External reference source/frequency and oscillator DAC | `C:device/m2_lm6_1/m2_lm6_1.c`, reference-clock setters, `...vctcxo_set`, initialization `extclk`/`extref` | Planned clock-source API | uSDR / attached front end | Missing / None / U | Current board uses fixed 26 MHz reference; overrides become typed choices. |
| Clock measurement / oscillator discipline | same C file, `...clkmeas_set`, `...clkmeas_get`; `tools:usdr_dm_gpsdo.c` feedback loop | Planned measurement and optional discipline policy | uSDR / reference-capable variants | Missing / None / U | Preserve functional measurement/control; CLI/PID client compatibility not required. |
| LO-leakage / IQ-imbalance calibration and results | same C file, `usdr_device_m2_lm6_1_calibrate_set`; `C:cal/cal_lo_iqimb.c` | Planned calibrated RX/TX controls | uSDR, internal/external feedback | Missing / None / U | Existing startup DC/LPF calibration retained; RF outcome cannot be qualified by register trace alone. |

## Shared capabilities and all-family expansion

| Capability | Pinned C evidence | Rust owner/interface | Variant / path | Implementation / software / hardware | Blocker or boundary |
|---|---|---|---|---|---|
| Discovery, identity/revision, implemented capability queries | `C:models/dm_dev.c::usdr_dmd_discovery`; board `/dm/revision`; `C:device/device_vfs.c` | Planned typed discovery/identity/capabilities | All registered families; supported transports | Partial uSDR transport selection / None for full API / U | Separate genuine operations from dummy success and declared-only names. |
| Front-end board probing/control | `C:device/device_fe.c::device_fe_probe`; `ext_exm2pe`, `ext_pciefe`, `ext_simplesync`, `ext_supersync`, `ext_fe_100_5000`, `ext_xmass`, `ext_fe_ch4_400_7200` | Planned front-end drivers, extracted at actual sharing seams | Compatible boards only | Missing / None / U | Do not advertise every front end on every family. |
| Alternate sample formats and channel mapping | `C:ipblks/streams/stream_sfetrx4_dma32.c::initialize_stream_rx_32`, `initialize_stream_tx_32`; `C:xdsp/conv.c` | Planned typed stream formats/maps | Family-specific CI12/CI16/F32 and channel counts | Partial CI16 channel0 / None for alternatives / U | Per-family valid maps and real/complex packing; simulator currently ignores these format bits. |
| Pollable stream FD / host-ready / RX throttle / TX mute | `C:models/dm_stream.c::{usdr_dms_get_fd,usdr_dms_set_ready}`; `stream_sfetrx4_dma32.c::_sfetrx4_option_set` | Planned reactor/stream controls | Supporting transports/cores | Partial internal RX ready / None for full API / U | Typed supported capabilities, no string-option compatibility. |
| Detailed TX telemetry and RX extended metadata | `C:models/dm_stream.h::usdr_dms_send_stat`, `usdr_dms_recv_nfo`; `dm_stream.c::usdr_dms_send_stat` | Planned stream telemetry | Supporting stream cores | Partial RX packet/loss fields / None for extra telemetry / U | FIFO/underrun/hardware timing/parts separate from existing cumulative loss. |
| Trigger selection/generated SYSREF/duplex start | `C:ipblks/streams/stream_sfetrx4_dma32.c::sfetrx4_stream_sync` | Planned typed synchronization | Families exposing sync cores | Partial free-run/off / None for trigger modes / U | SYSREF, 1PPS, RX/TX/either/both activity are distinct modes. |
| Multi-device control broadcast/channel aggregation | `C:device/mdev.c::_mdev_obj_set_i64`, `_mdev_create_stream`, `_mstr_stream_recv`, `_mstr_stream_send` | Planned SDR aggregate, once second driver is present | Compatible homogeneous devices; POSIX reference path | Missing / None / U | C reports master metadata and has aggregation TODOs; Rust must expose partial failures, not claim atomic broadcast. |
| Multi-device synchronized start/master trigger | `C:device/mdev.c::_mdev_stream_sync` | Planned aggregate synchronization | Reference supports up to two stream groups | Missing / None / U | Arm members before master generator; reference rejects direct aggregate `sysref+gen`. |
| Integer/float/packed sample conversion and interleaving | `C:xdsp/conv.c`, conversion files in `xdsp/CMakeLists.txt` | Planned reusable DSP/conversion owner | Scalar first; supported host/wire formats | Missing / None / U | Preserve saturation/scaling/rounding and odd/boundary lengths before SIMD. |
| FIR / combined conversion-filter kernels | `C:xdsp/filter.c`, `conv_filter.c` | Planned shared DSP | Supported channel/rate plans | Missing / None / U | Existing FPGA FIR programming is retained but not these host kernels. |
| FFT / windows / FFTAD | `C:xdsp/intfft.c`, `fft_window_functions.c`, `fftad_functions.c` | Planned shared DSP | Architecture-dependent reference build | Missing / None / U | This is a distinct usdr-dsp domain, not board-private RX code. |
| NCO / trig / sine/chirp generation | `C:xdsp/nco.c`, `trig.c`, `sincos_functions.c`; `tools:usdr_dm_create.c` | Planned shared DSP/generator primitives | Host DSP/TX clients | Missing / None / U | Benchmarked SIMD only after scalar reference parity. |
| RTSA / FM quadrature | `C:xdsp/rtsa_functions.c`, `fmquad.c` | Planned shared DSP | Host analysis | Missing / None / U | Functional algorithms retained; legacy daemon commands are not evidence they are implemented. |
| Calibration search / sample fitting and statistics | `C:cal/opt_func.c`, `cal_filt.c`; `cal_filt.h` implemented routines | Planned shared calibration | Applicable host samples/chips/families | Missing / None / U | Packed conversion, DC removal, statistics, IQ generation and phase/period fitting; this file does not establish hardware filter calibration. |
| LMS7-family board/RX/TX/rate controls | `C:device/m2_lm7_1/{m2_lm7_1,xsdr_ctrl,lms7002m_ctrl}.c` | Planned driver and LMS7002M/LMS8001 chip owners | XSDR, XTRX, SSDR, SSDRPRO variants | Missing / None / U | Factory/HW IDs govern capability matrix; TDD is an additional operation. |
| LimeSDR board / framing / streams | `C:device/u3_limesdr/{u3_limesdr,limesdr_ctrl,limesdr_fpga}.c`; `C:ipblks/streams/stream_limesdr.c` | Planned LimeSDR driver sharing LMS7002M | Registered LimeSDR-mini IDs; FT601 USB | Missing / None / U | FT601 protocol is not URAM with a different VID/PID. |
| LSDr ADC/clock/power/RX driver | `C:device/m2_lsdr/m2_da09_4_ad45_2.c` | Planned LSDr driver/chips | Factory m2_d09_4_ad45_2; evidence-led transport map | Missing / None / U | ADS42LBx9, Si549, TPS6594, LMK04832; do not invent TX support. |
| DSDR JESD/AFE/rates/multiband RX/TX | `C:device/m2_dsdr/m2_dsdr.c`; `C:hw/afe79xx/afe79xx.c::afe79xx_create`, `afe79xx_init` | Planned DSDR and pure-Rust AFE owner | Board/JESD/profile-specific, up to eight logical channels | Blocked / None / U | NDA CAPI implementation and external configuration profiles missing. Requires authorized register/profile evidence; no C plugin fallback, SKIPAFE dummy or assumed successful port. |
| PE_SYNC clock tree / distribution / synchronization | `C:device/pe_sync/pe_sync.c` | Planned synchronization device driver | PE_SYNC, USB-supported ID | Missing / None / U | LMK05318/LMX2820/LMX1214/LMX1204/LMK1D1208I; not an SDR sample source. |
| FT601 USB transport | `C:lowlevel/usb_ft601/usb_ft601_{generic,libusb}.c` | Planned safe protocol over rsl-usb | LimeSDR registered IDs; Linux first | Missing / None / U | Independent framing, endpoint setup and failure semantics. |
| Additional-family PCIe layouts | `C:lowlevel/pcie_uram/pcie_uram_main.c`; family device-bus declarations | Planned layouts over rsl-usdr-pcie | Linux, existing kernel ABI | Partial uSDR layout / None for other layouts / U | Preserve kernel boundary; driver rewrite is not required. |
| WebUSB / non-Linux USB backends | `C:lowlevel/usb_uram/usb_uram_webusb.c`, FT601 WebUSB; `C:webusb/CMakeLists.txt`; libusb platform branches | Planned host backends over common protocol | Evidence-led browser/macOS/Windows phases | Missing / None / U | Linux-first does not mean later platforms are qualified; no libusb C runtime dependency. |
| Verilator host bridge | `C:lowlevel/verilator_ll/{verilatorll_wrap,unix_vll,vpu}.c` | Planned optional simulator transport | POSIX simulation | Missing / None / U | Distinct from current register-level Rust simulator; gateware simulator is external. |
| Typed debug/raw-register/diagnostic access | `C:models/dm_debug.c`; board debug handlers | Planned bounded diagnostic API | Variant-supported buses/chips | Partial lowlevel Bus / None for full typed service / U | Preserve functional access; no requirement to reproduce socket commands or JSON serialization. |
| Flash ID/read/erase/write and image metadata | `C:ipblks/espi_flash.c`; `tools:usdr_flash.c`, `xmass_fwupd.c` | Planned maintenance API, separate destructive actions | Flash/front-end variants with source evidence | Missing / None / U | Golden/master headers, alignment/checksums and recovery need explicit tests; actual flashing requires separate authorization. |
| General time API declarations | `C:models/dm_time.h`; empty `dm_time.c` at PIN | Capability reports declared-only unsupported operations | None implemented by that file | Reference declaration only / None / U | Do not fabricate a clock service; implemented stream timing remains separately in scope. |
| Entity filter/notify declarations and dummy success | `C:models/dm_dev.h` disabled declarations; `C:device/m2_dsdr/m2_dsdr.c::dev_m2_dsdr_dummy` | Typed unsupported/capability results | Affected API/family | Reference unsupported/no-op / None / U | Successful no-op is not functional parity; do not enable disabled C callbacks by inference. |
| TIA feedback declarations / stub DC measurement / mock antenna timing | `C:device/m2_lm6_1/m2_lm6_1.c`, `...rx_tia_cfb_set`, `...rx_tia_rfb_set`, `...rx_dc_meas_get`, `...tx_antennat_port_cfg_set` | Typed unsupported/capability results | uSDR | Reference unsupported/no-op / None / U | TIA code disabled and returns -EINVAL; DC measurement returns constant0 with reads disabled; antenna timing explicitly mockup returning0. Do not count these as functional features to port. |

Legacy C ABI, Python/Soapy clients, VFS aliases, JSON/base64 RPC framing, daemon method
names and CLI option compatibility are excluded. Underlying implemented operations above
remain in scope. Merely listed daemon requests are not capabilities: several names in
`json_controller/controller.c` have no matching implementation. Firmware images, FPGA
gateware and Linux kernel code remain external dependencies rather than Rust library code.

## Approved execution order

1. **Execution foundation — In progress.** Preserve the existing `Device`/`Bus` baseline
   while extracting real uSDR sequences into typed, bounded sans-I/O state machines.
   Portable-core-first is now part of this phase: migrated framing, register/divider maths
   and machines must compile without std through the exact-source witness. Core uses
   caller-supplied time and bounded actions/events; native adapters preserve OS sources and
   contextual errors. Allocation dependencies are allowed, while fixed machines remain
   free of per-operation allocation. Whole board/device migration is not a prerequisite.
   The temperature/I²C pilot now runs in the production blocking path and under test-only
   readiness, managed-thread/future and callback runners. Generic USB now has bounded reap
   and filed-only completion primitives plus a tested whole-queue cleanup-worker handoff.
   Cancellation itself can block in the kernel; it must not run on the reactor. Generic USB
   readiness registration now exists (`rsl_usb::Readiness`). The next gate is the uSDR
   adapter over it, blocking-syscall isolation and externally driven
   lifecycle/worker supervision; then migrate
   power/current RX and introduce public independently owned stream handles. Real token
   retirement, fairness, timer scheduling and browser integration are not established by
   fake-source tests. Managed adapters must remain opt-in consumers of that same engine.
2. **Complete uSDR RX/control/clock/calibration.** Port remaining per-stage gain, path,
   corrections, independent rate/NCO plans, external clocks and calibration results in
   small ledger-row changes. Retain temperature, startup DC/LPF, tuning and stream parity.
3. **Complete uSDR TX and stream semantics.** Add TX controls then USB/PCIe transmission,
   timestamps/telemetry, formats/channel maps, flow control and trigger modes. Complete
   uSDR software parity before declaring a second family supported.
4. **Shared functional library.** Finish discovery/capability/diagnostic/maintenance APIs,
   full scalar DSP/calibration domains, aggregate control/stream synchronization and
   reusable front-end support. Extract sharing only where concrete consumers establish it.
   Hardware-free maintenance tests use images/fake flash, never real erase/write.
5. **All registered families.** Port LMS7-family variants, then LimeSDR/FT601, LSDr and
   PE_SYNC; sequence DSDR behind its evidence prerequisite, not a stub. Each family gets
   a chip/register simulator, exact variant/transport matrix and its own reference tests.
6. **Platform and accelerated implementations.** After Linux software closure, add the
   reference's other host/transport combinations that have demonstrated implementation
   evidence, including WebUSB and optional Verilator bridge. SIMD follows scalar parity
   and a measured bottleneck. Scalar semantics remain authoritative and available as the
   fallback. Optimized DSP/kernel leaves live beside their owner and require differential
   tests; USB/PCIe/syscall/queue/scheduling tuning stays in host transports/executors. Do not
   put OS/runtime dependencies into portable core to enable acceleration. No publication
   or hardware claim follows from this phase.

## Kernel and deployment boundary

USB uses Linux's existing usbfs/USB host stack and needs device-node access, not a custom
uSDR kernel module. See [Linux's userspace USB interface](https://docs.kernel.org/driver-api/usb/usb.html).
PCIe uses the external **C** `usdr_pcie_uram` kernel module, ABI3, for device nodes, BAR
mapping, interrupts and DMA. It must be installed/loaded separately from Cargo; consult
[upstream module installation/build guidance](https://github.com/wavelet-lab/usdr-lib#pcie-driver)
for DKMS or kernel-header builds and deployment-specific module signing. No library API
may install/load it automatically. Existing typed open/version errors and compiled-header
ABI tests remain the software boundary; actual kernel/DMA behavior stays unqualified.

The Rust userspace-library replacement does not replace that C kernel module. A Rust
kernel rewrite, privileged installation, firmware/FPGA changes and embedded deployment
require separate scope/authority. Portability compilation alone cannot justify them.

## Acceptance and review loop

For every row/variant: inspect its pinned implementation and specification dependencies;
identify supported/no-op/declared-only behaviour; write independent C-oracle or
specification fixtures; implement the smallest typed Rust operation; add deterministic
failure/cancellation/boundedness tests; review correctness, public API and layering;
resolve findings; then update the row with exact interface/test names and divergences.
Each migrated core operation also extends the exact-source portability witness; native
and fixture dependency declarations must remain consistent, with no substitute algorithms.
Only status actually established changes. Reviewer findings return to the same row;
software parity alone never promotes hardware status. New variants need their own evidence.

Qualification is layered, not a repeated full-workspace ritual:

- **Fast loop:** affected core/codec/chip tests, fake clock and owned-buffer boundary;
  fmt and focused package lint. No device open, networking or wall-clock sleeps.
- **Software closure:** affected default/feature configurations and downstream consumers,
  C parity, state-machine/cancellation faults, docs/examples and variant capability checks.
  Maintain packet/gain/rate/format boundary and scalar-vs-reference oracles.
  Portable-core milestone acceptance additionally requires an explicit local target check
  of the shared production tree for thumbv7em-none-eabi; qualify only compilation of the
  stated subset with alloc, not linking or the whole public crate.
- **Release qualification:** affected parsers/unsafe seams get the repository's scoped
  fuzz tier; API/MSRV/platform matrices, ownership stress and measured DSP/stream benchmarks
  follow the existing release policy. This roadmap does not authorize running them now.
  Ordinary CI adds only the selected fixture's host check/lint/tests and manifest contract;
  bare-metal CI requires an explicitly releasing USDR product, not merely a dependency's
  release closure reaching the fixture. USDR release opt-ins remain unchanged.
- **Publication:** preserve repository pre-push/hosted gates; publish only on a separately
  authorized route. `publish = false` stays unchanged.
- **Physical qualification:** not now. Later explicit opt-in hardware fixtures must record
  board/revision/gateware/kernel/transport, thermal/clock/RF outcomes, loss/underrun,
  cleanup/restart and maintenance safety. No tests here were run on hardware.

Current simulators explicitly omit RF signals, timing/overruns and alternate formats;
USB reference tests substitute the host-buffer boundary, and PCIe tests establish ABI
shape, not DMA behaviour. These blind spots are open qualification work, not silent success.
AFE79xx remains blocked until authorized implementation/profile/register evidence and a
valid independent oracle exist; unrelated phases may proceed without changing that status.
