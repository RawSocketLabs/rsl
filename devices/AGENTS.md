# devices — agent & contributor guide

> `CLAUDE.md` is a symlink to this file. Inherits the workspace-root `../AGENTS.md`; each
> driver carries its own `AGENTS.md` with driver-specific detail.

**What this is.** Pure-Rust drivers for hardware, grouped by kind: `devices/<kind>/<device>`,
for example `sdr/usdr`. A driver's simulator, oracle and driver-interface crates live inside
its directory (`sdr/usdr/{sim,pcie,oracle}`).

## Layout rules

- A new driver goes under the kind it is (`sdr/`, …); add a kind when the first device of it
  arrives. Crate names stay `rsl-<device>` (`usdr`), independent of the path.
- Kinds now: `sdr/` (uSDR, LMS7 family, LSDr, LimeSDR) and `sync/` (PE_SYNC clock distribution).
- Code shared by several drivers of one kind (an SDR tune/rate/stream abstraction, say)
  lives in `devices/<kind>/` once a second driver needs it, not before. The first is
  `sdr/frontend` (`sdr-frontend`, 2026-10-13): libusdr's front-end probe
  (`device/device_fe.c`), every board's identification and the bring-ups the probe needs
  whole (`pciefe`, `exm2pe`), run by the uSDR, XSDR and LSDr drivers. A driver adds the
  bring-ups it owns through its `BringUp` (the uSDR's XMASS, `SuperSync` and `SimpleSync`)
  and re-exports the crate's types.
- Code every kind's drivers share lives in `devices/lowlevel` (`rsl-lowlevel`): libusdr's
  `ls_op` seam (`Bus`, `BusError`, I2C/SPI addresses), the typed-register traits and the
  portable `Sequence` seam with its blocking executor. It is `no_std` (std items behind
  feature `std`), knows no board, chip or transport crate, and depends on nothing owned;
  board-specific limits (which I2C cores or buses exist) stay in the driver's transports.
- Shared chip code goes under `devices/<kind>/chips/<chip>/`, one directory per chip: the
  chip's register values and identity facts are the crate at the directory itself
  (`sdr/chips/lp8758`: `lp8758`, `no_std`, on `rsl-lowlevel` and `bnb`), and per-chip
  crate roles sit beside it (`sdr/chips/lms7002m/sim`: `lms7002m-sim`, the LMS7002M
  model the XSDR and LimeSDR sims share). Board wiring, setpoints and sequences stay in
  the drivers. A driver keeps stable internal paths by re-exporting such a crate.
- Chip code only one driver uses stays in that driver's `src/portable/<chip>.rs` until a
  second driver needs it, then moves to its chip crate: the LSDr's TPS6594, Si549,
  ADS42LBx9 and LMK04832 sit in `lsdr` (2026-10-12; libusdr uses them on no other
  board), and PE_SYNC's LMX2820, LMX1204, LMX1214 and LMK1D1208I sit in `pe-sync`
  (2026-10-13; likewise). The chips only front-end boards carry (TCA6416, TCA9555,
  `DACx0501`) sit in `sdr-frontend` until a driver needs one directly (the XTRX's
  DAC80501 trim would move it to `sdr/chips/dac80501`).
- A chip drivers of both kinds use stays under the kind that used it first:
  `sdr/chips/lmk05318` (`lmk05318`) serves the uSDR's front ends and PE_SYNC (and
  `sdr-frontend`'s `SimpleSync` ID check).
- A chip crate may also hold the chip's own operations as sans-I/O sequences, when they
  are the vendor library's chip-driver code and know no board: `lms7002m` ports
  libusdr's `hw/lms7002m/lms7002m.c` (`Create`, and the `Lms7002m` cached state whose
  operations return write `Batch`es). libusdr's `device/m2_lm7_1/lms7002m_ctrl.c` is
  LMS7002M control code too, despite its directory (the LimeSDR's `limesdr_ctrl.c` calls
  its `lms7002m_streaming_up`): its functions belong in `lms7002m` once a second
  driver needs them; until then a port of one may sit in its first driver
  (`lms7002m_streaming_down` in `xsdr`'s `Close`). Those both LMS7 drivers call
  (`lms7002m_samplerate`, `_set_gain`, `_bb_set_badwidth`, `_fe_set_freq`,
  `_streaming_up`) are `lms7002m::ctrl` (2026-10-13): `Control` is
  `lms7002_dev_t`, each operation a sans-I/O `Operation`, and what libusdr's callbacks
  ask of the board (antenna switches, `LimeLight` sample order) its `Frontend`. Board
  composition stays in drivers.
- Transports any device can use (raw sockets, USB) are not devices: they live at the
  workspace root (`transports/rawsock/`, …), and drivers depend on them.
- FFI crates wrapping a vendor's C library stay excluded under `bindings/` (`bindings/usdr/`).
