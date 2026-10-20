# RTL-SDR (RTL2832U + R820T2/R828D)

> Inherits `../../AGENTS.md` (devices) and the workspace-root `../../../AGENTS.md`.
> `CLAUDE.md` is a symlink to this file.

A **clean-room** pure-Rust driver for RTL2832U RTL-SDR dongles, its dongle simulator and its
capture replay harness. Unlike the libusdr ports beside it there is no reference oracle:
every fact comes from an admissible source in [PROVENANCE.md](PROVENANCE.md), and the
RTL2832U side waits on captures of the owner's dongles ([CAPTURES.md](CAPTURES.md)).
Nothing here is hardware-qualified.

## Licensing rule (owner's, absolute)

Target licence MIT OR Apache-2.0. Never open, read, grep, search for or quote any GPL
RTL-SDR source or anything derived from one: librtlsdr/osmocom rtl-sdr and forks, the
Linux kernel's rtl28xxu/rtl2832/r820t/e4000/fc001x drivers, rtl-sdr-rs, pyrtlsdr
internals, SDR++/gqrx code. Recollection of those sources is not evidence. Every register
address, value, request code, identifier and sequence must have a ledger row with an
admissible source (classes in PROVENANCE.md); every such constant carries a `prov: <ID>`
doc tag. A value you lack is a GAP row and a `Pending` placeholder that refuses at runtime,
never a guessed number. Vendor documents stay outside the repo; cite them by page/table.

| Path | Crate | Role |
|------|-------|------|
| `.` | `rtlsdr` (unpublished, root workspace member) | The driver, on `hal` (`BusError`) and, behind feature `usb`, `usb` |
| `sim/` | `rtlsdr-sim` (unpublished, root workspace member) | Dongle model at the control seam: a generic register store, an R820T2 register file behind an optional gate bit (shape assumed, `RTL-GAP-13`), routed by a caller-supplied function of the setup (`RTL-GAP-14`), a bulk RX feed, virtual clock, op trace; and the `Trace` format and `Replay` harness |

## Driver layout

- `src/control.rs`: `ControlPort`, the seam: vendor control IN/OUT, a bulk RX stream
  (`rx_start`/`rx_read`/`rx_stop`), sleep; that shape is assumed (`RTL-GAP-12`). Like
  `limesdr`'s, it is not `hal::Bus`: which transfers carry which access is the driver's
  (all GAP rows so far).
- `src/device.rs`: `Device`, whose `open` refuses with `RTL-GAP-07` before touching the
  port.
- `src/transport/usb.rs` (feature `usb`, Linux): `UsbPort`, the seam over `usb`. The caller
  passes VID:PID (or a `usb::DeviceInfo` picked by serial or port path), interface and bulk
  IN endpoint; none is hard-coded until DESC rows exist.
- `src/portable/`: no I/O, imports within the tree (no `no_std` witness yet).
  `pending.rs`: `Pending<T>`/`Gap`. `rtl2832u.rs`: the demodulator's placeholders.
  `r820t2/`: the tuner, from the register description only — `registers` (I2C framing,
  `Registers` image, page writes, LSB-first reads; only registers read from the chip may
  be written, `R820-GAP-08`), `field` (every field T1-2 and T1-3 name), `setting` (typed
  enumerated settings, `FieldValue`), `pll` (`Ndiv` from `SI2C`/`NI2C`/`SDM_IN`), `gain` (IF VGA steps). Chip
  code stays here while `rtlsdr` is its only consumer (`../../AGENTS.md`). Typed values
  are per field, not `hal::Register` whole-byte types: those need T1-2's fixed bits, whose
  status is `R820-GAP-08`.
- `tests/common/`: `SimPort` and `ReplayPort` (the seam over the sim and over a `Replay`),
  `wire.rs` (their shared setup conversion) and `synthetic.rs`, the SYNTHETIC wiring and
  session; included by `#[path]`. `tests/provenance.rs` checks the ledger against the code.
- `tests/fixtures/synthetic-{session,stall}.trace`: the only committed traces, SYNTHETIC.

## Rules

- Tests cite the ledger row their vector comes from. The sim's tuner model is written from
  the document independently of `src/portable/r820t2`, so the seam tests cross-check them
  (except where both rest on one reading of the document: `R820-DS-04`).
- Captures and traces derived from them stay out of the repo until the owner approves
  (CAPTURES.md). A GPLOBS capture is never a row's sole source.
- Hardware tests are `#[ignore]`d and need the env vars in `tests/usb_hardware.rs`; blacklist
  `dvb_usb_rtl28xxu` first.

## Verify

```sh
cargo clippy -p rtlsdr -p rtlsdr-sim --all-targets --all-features -- -D warnings
cargo test -p rtlsdr -p rtlsdr-sim --all-features
RSL_RTLSDR_USB_DEVICE=vvvv:pppp RSL_RTLSDR_USB_INTERFACE=n RSL_RTLSDR_USB_RX_ENDPOINT=0xNN \
    cargo test -p rtlsdr --features usb --test usb_hardware -- --ignored   # real dongle
```
