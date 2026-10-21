# devices — agent & contributor guide

> `CLAUDE.md` is a symlink to this file. Inherits the workspace-root `../AGENTS.md`; each
> driver carries its own `AGENTS.md` with driver-specific detail.

**What this is.** Pure-Rust drivers for hardware, grouped by kind: `devices/<kind>/<device>`,
for example `sdr/usdr`. A driver's simulator, oracle and driver-interface crates live inside
its directory (`sdr/usdr/{sim,pcie,oracle}`). All of them are unpublished.

## Layout

| Path | Crate(s) | Notes |
|------|----------|-------|
| `hal/` | `hal` | the drivers' shared `ls_op` seam: bus addresses, typed registers, portable `Sequence`s; `no_std`, `Bus`/`BusError`/blocking executor behind `std` |
| `chips/lmk05318/` | `lmk05318` | the LMK05318 register values and frequency plans the front-end boards (`sdr-frontend`) and PE_SYNC share (`no_std`) |
| `vendor/` | none | `libusdr/`, the vendored libusdr C subset every oracle compiles (one tree, one PIN), and `vendor.sh`, its refresh script |
| `sdr/usdr/` | `usdr`, `usdr-sim`, `usdr-pcie`, `usdr-oracle`, `usdr-nostd-check` | the uSDR driver (unpublished until parity), its board simulator, its PCIe driver interface, the libusdr parity oracle (own workspace: compiles the vendored C) and its `no_std` portable-tree witness |
| `sdr/xsdr/` | `xsdr`, `xsdr-sim`, `xsdr-oracle`, `xsdr-nostd-check` | the LMS7 family driver: XSDR open with its front-end probe and bring-up, RFIC power-up, sample rate, LO tuning, gains, bandwidths, paths and loopbacks, NCOs, corrections, the RX stream and close; oracle and witness as above |
| `sdr/lsdr/` | `lsdr`, `lsdr-sim`, `lsdr-oracle`, `lsdr-nostd-check` | the LSDr (receive-only ADC board) driver: open with its front-end probe and fe1005000's bring-up, ADC power, PGA gain and close |
| `sdr/limesdr/` | `limesdr`, `limesdr-sim`, `limesdr-oracle`, `limesdr-nostd-check` | the LimeSDR-mini driver: open over its LMS64C control channel, power-up, sample rate, LO tuning, gains, bandwidths, stream preparation and close; no FT601 transport yet |
| `sdr/rtlsdr/` | `rtlsdr`, `rtlsdr-sim` | the clean-room RTL2832U RTL-SDR driver (no oracle: every value from `PROVENANCE.md`): its control seam with a usbfs port, R820T2 register maths from the register description, and the dongle simulator with the capture replay harness; the RTL2832U side awaits captures |
| `sdr/chips/lms7002m/` | `lms7002m`, `lms7002m-sim` (`sim/`) | the LMS7002M register values and chip sequences (`no_std`), and the LMS7002M model the XSDR and LimeSDR sims share |
| `sdr/chips/lp8758/` | `lp8758` | the LP8758 PMIC register values (`no_std`) |
| `sdr/frontend/` | `sdr-frontend`, `sdr-frontend-sim` (`sim/`) | libusdr's front-end board probe (`device_fe.c`: order, `fe=` hints, every board's identification) and every bring-up it runs on the uSDR, XSDR and LSDr (`pciefe`, `xmass`, `supersync`, `simplesync`, `exm2pe`, `fe1005000`) with the boards' controls, shared by those drivers (`no_std` core, blocking probe behind `std`); and the M.2 boards' register models the uSDR and XSDR sims share |
| `sdr/stream/` | `sdr-stream`, `sdr-stream-sim` (`sim/`) | the FPGA stream engine the uSDR and LMS7 family gateware share at the same addresses (`ipblks/streams/`): its register sequences (`EngineSequence`: RX DMA and front end, sync unit, TX cores), RX burst arithmetic (`BurstPlan`) and the original TX core's announcement and status, `no_std`; and the RX engine, TX core and sync unit models the uSDR and XSDR sims share |
| `sdr/gearbox/` | `sdr-gearbox` | libusdr's FPGA FIR gearbox microcode (`ipblks/fgearbox.c`: each 7-series decimation and interpolation table), moved out of `usdr` when the XSDR's DUC/DDC gateware needed it; `no_std`, no dependencies |
| `sdr/cal/` | `sdr-cal` | libusdr's calibration mathematics (`cal/opt_func.c` integer searches, `cal/cal_filt.c` sinusoid fits), moved out of `usdr` (which re-exports it as `usdr::cal`) when the XSDR's TX quality check needed the fit; `std` (floating-point `sin`/`cos`) |
| `sync/pe_sync/` | `pe-sync`, `pe-sync-sim`, `pe-sync-oracle`, `pe-sync-nostd-check` | the PE_SYNC clock-distribution board driver: libusdr's whole open and close |

## Layout rules

- A new driver goes under the kind it is (`sdr/`, …); add a kind when the first device of it
  arrives. A crate is named for its device (`usdr`, `usdr-sim`), independent of the path,
  with no `rsl-` prefix.
- Kinds now: `sdr/` (uSDR, LMS7 family, LSDr, LimeSDR, RTL-SDR) and `sync/` (PE_SYNC clock distribution).
- Code shared by several drivers of one kind (an SDR tune/rate/stream abstraction, say)
  lives in `devices/<kind>/` once a second driver needs it, not before. The first is
  `sdr/frontend` (`sdr-frontend`, 2026-10-13): libusdr's front-end probe
  (`device/device_fe.c`), every board's identification and every board's bring-up and
  controls the probe's drivers run (`pciefe`, `xmass`, `supersync`, `simplesync`,
  `exm2pe`, `fe1005000`; 2026-10-18), run by the uSDR, XSDR and LSDr drivers whose
  libusdr probes their connector. `Connector::probe` brings the board up whole and hands
  the driver the `FrontEnd` libusdr keeps (`struct dev_fe`); the driver keeps it, offers
  the controls its libusdr device offers, and re-exports the crate's types. A board whose
  bring-up is not ported stops the probe (`Error::NotPorted`, fe4ch4007200 only). The
  boards' register models live beside it (`sdr/frontend/sim`, `sdr-frontend-sim`), shared
  by the sims whose connector admits them (`usdr-sim`, `xsdr-sim`); a board only one
  driver admits keeps its model in that driver's sim (fe1005000 in `lsdr-sim`).
  The second is `sdr/stream` (`sdr-stream`, 2026-10-18), the stream engine's registers
  and burst arithmetic, moved out of `usdr` (which re-exports them where its board and
  `NativeUsb` found them) when the XSDR's RX streams needed them; which logical channel
  rides which lanes, host formats and the RF side of a stream stay in each driver. Its
  sim model (`sdr/stream/sim`, `sdr-stream-sim`: the RX engine and sync unit) is shared
  likewise. The XSDR's TX streams (2026-10-19) moved the original TX core's announcement
  words, status decoding and create checks into `sdr-stream` (re-exported by `usdr`'s
  `fpga/tx_stream.rs`) and the TX core model with its transport buffers into
  `sdr-stream-sim` (re-exported by `usdr-sim`); the extended TX core's announcement cache
  followed (2026-10-21) when the XSDR's extended TX front end streamed on it. The third is
  `sdr/cal` (`sdr-cal`, 2026-10-19): libusdr's calibration mathematics, `usdr`'s
  `cal::{fit, search}` (re-exported there), when the XSDR's TX quality check needed the
  fit; the fit uses the search, so both moved. The fourth is `sdr/gearbox`
  (`sdr-gearbox`, 2026-10-21): the FPGA FIR gearbox's microcode tables, `usdr`'s
  `portable/fir_tables.rs`, when the XSDR's DUC/DDC gateware needed them (the LSDr will
  too); each driver keeps its own loader, as their write targets and gaps differ.
- Code every kind's drivers share lives in `devices/hal` (`hal`): libusdr's
  `ls_op` seam (`Bus`, `BusError`, I2C/SPI addresses), the typed-register traits and the
  portable `Sequence` seam with its blocking executor. It is `no_std` (std items behind
  feature `std`), knows no board, chip or transport crate, and depends on nothing owned;
  board-specific limits (which I2C cores or buses exist) stay in the driver's transports.
- Shared chip code goes under `devices/<kind>/chips/<chip>/`, one directory per chip: the
  chip's register values and identity facts are the crate at the directory itself
  (`sdr/chips/lp8758`: `lp8758`, `no_std`, on `hal` and `bnb`), and per-chip
  crate roles sit beside it (`sdr/chips/lms7002m/sim`: `lms7002m-sim`, the LMS7002M
  model the XSDR and LimeSDR sims share). Board wiring, setpoints and sequences stay in
  the drivers. A driver keeps stable internal paths by re-exporting such a crate.
- Chip code only one driver uses stays in that driver's `src/portable/<chip>.rs` until a
  second driver needs it, then moves to its chip crate: the LSDr's TPS6594, Si549,
  ADS42LBx9 and LMK04832 sit in `lsdr` (2026-10-12; libusdr uses them on no other
  board), and PE_SYNC's LMX2820, LMX1204, LMX1214 and LMK1D1208I sit in `pe-sync`
  (2026-10-13; likewise). The chips only front-end boards carry (TCA6416, TCA9555,
  `DACx0501`, LMK5C33216, AT24, and a small TMP114) sit in `sdr-frontend` until a driver
  needs one directly (the XTRX's DAC80501 trim would move it to `sdr/chips/dac80501`, and
  its sim model, now `sdr-frontend-sim`'s beside `xsdr-sim`'s own, to
  `sdr/chips/dac80501/sim`).
- A chip drivers of more than one kind use goes under `devices/chips/<chip>/`:
  `chips/lmk05318` (`lmk05318`) serves the front-end boards' bring-ups (`sdr-frontend`,
  whose `chips::lmk05318` is the bus code over its plans) and PE_SYNC. Its sim model is
  still two (`sdr-frontend-sim`'s and `pe-sync-sim`'s, on the same evidence but each on
  its sim's register-file seam); joining them as `chips/lmk05318/sim` is open.
- A chip crate may also hold the chip's own operations as sans-I/O sequences, when they
  are the vendor library's chip-driver code and know no board: `lms7002m` ports
  libusdr's `hw/lms7002m/lms7002m.c` (`Create`, and the `Lms7002m` cached state whose
  operations return write `Batch`es). libusdr's `device/m2_lm7_1/lms7002m_ctrl.c` is
  LMS7002M control code too, despite its directory (the LimeSDR's `limesdr_ctrl.c` calls
  its `lms7002m_streaming_up`): its functions belong in `lms7002m` once a second
  driver needs them; until then a port of one may sit in its first driver
  (`lms7002m_streaming_down` in `xsdr`'s `portable/close.rs`, `streaming_down`, which
  its `Close` and its RX stream's destroy both build). Those both LMS7 drivers call
  (`lms7002m_samplerate`, `_set_gain`, `_bb_set_badwidth`, `_fe_set_freq`,
  `_streaming_up`) are `lms7002m::ctrl` (2026-10-13), and so are the paths and NCOs
  (`_rfe_set_path`, `_tfe_set_path`, `_fe_set_lna`, `_bb_set_freq`), which only the
  XSDR offers so far but which live on the same state: `Control` is
  `lms7002_dev_t`, each operation a sans-I/O `Operation`, and what libusdr's callbacks
  ask of the board (antenna switches, `LimeLight` sample order) its `Frontend`. Board
  composition stays in drivers.
- Vendored vendor-library C lives under `devices/vendor/`, not inside one oracle: there is
  one libusdr tree and one PIN (`vendor/libusdr/PIN`), compiled by every oracle by relative
  path. Never edit it by hand; refresh it with `vendor/vendor.sh`, and bump the PIN in its
  own change.
- Transports any device can use (raw sockets, USB) are not devices: they live under
  `transports/` at the workspace root (`transports/rawsock`, `transports/usb`), and drivers
  depend on them.
- FFI crates wrapping a vendor's C library stay excluded under `bindings/` (`bindings/usdr/`,
  crate `usdr-ffi`).
