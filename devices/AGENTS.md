# devices — agent & contributor guide

> `CLAUDE.md` is a symlink to this file. Inherits the workspace-root `../AGENTS.md`; each
> driver carries its own `AGENTS.md` with driver-specific detail.

**What this is.** Pure-Rust drivers for hardware, grouped by kind: `devices/<kind>/<device>`,
for example `sdr/usdr`. A driver's simulator, oracle and driver-interface crates live inside
its directory (`sdr/usdr/{sim,pcie,oracle}`).

## Layout rules

- A new driver goes under the kind it is (`sdr/`, …); add a kind when the first device of it
  arrives. Crate names stay `rsl-<device>` (`rsl-usdr`), independent of the path.
- Kinds now: `sdr/` (uSDR, LMS7 family, LSDr, LimeSDR) and `sync/` (PE_SYNC clock distribution).
- Code shared by several drivers of one kind (an SDR tune/rate/stream abstraction, say)
  lives in `devices/<kind>/` once a second driver needs it, not before.
- Code every kind's drivers share lives in `devices/lowlevel` (`rsl-lowlevel`): libusdr's
  `ls_op` seam (`Bus`, `BusError`, I2C/SPI addresses), the typed-register traits and the
  portable `Sequence` seam with its blocking executor. It is `no_std` (std items behind
  feature `std`), knows no board, chip or transport crate, and depends on nothing owned;
  board-specific limits (which I2C cores or buses exist) stay in the driver's transports.
- Shared chip code goes under `devices/<kind>/chips/<chip>/`, one directory per chip: the
  chip's register values and identity facts are the crate at the directory itself
  (`sdr/chips/lp8758`: `rsl-lp8758`, `no_std`, on `rsl-lowlevel` and `bnb`), and per-chip
  crate roles sit beside it (`sdr/chips/lms7002m/sim`: `rsl-lms7002m-sim`, the LMS7002M
  model the XSDR and LimeSDR sims share). Board wiring, setpoints and sequences stay in
  the drivers. A driver keeps stable internal paths by re-exporting such a crate.
- Chip code only one driver uses stays in that driver's `src/portable/<chip>.rs` until a
  second driver needs it, then moves to its chip crate: the LSDr's TPS6594, Si549,
  ADS42LBx9 and LMK04832 sit in `rsl-lsdr` (2026-10-12; libusdr uses them on no other
  board).
- A chip crate may also hold the chip's own operations as sans-I/O sequences, when they
  are the vendor library's chip-driver code and know no board: `rsl-lms7002m` ports
  libusdr's `hw/lms7002m/lms7002m.c` (`Create`, and the `Lms7002m` cached state whose
  operations return write `Batch`es). libusdr's `device/m2_lm7_1/lms7002m_ctrl.c` is
  LMS7002M control code too, despite its directory (the LimeSDR's `limesdr_ctrl.c` calls
  its `lms7002m_streaming_up`): its functions belong in `rsl-lms7002m` once a second
  driver needs them; until then a port of one may sit in its first driver
  (`lms7002m_streaming_down` in `rsl-xsdr`'s `Close`). Board composition stays in drivers.
- Transports any device can use (raw sockets, USB) are not devices: they live at the
  workspace root (`rawsock/`, …), and drivers depend on them.
- FFI crates wrapping a vendor's C library stay excluded under `bindings/` (`bindings/usdr/`).
