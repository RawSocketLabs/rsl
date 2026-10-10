# devices — agent & contributor guide

> `CLAUDE.md` is a symlink to this file. Inherits the workspace-root `../AGENTS.md`; each
> driver carries its own `AGENTS.md` with driver-specific detail.

**What this is.** Pure-Rust drivers for hardware, grouped by kind: `devices/<kind>/<device>`,
for example `sdr/usdr`. A driver's simulator, oracle and driver-interface crates live inside
its directory (`sdr/usdr/{sim,pcie,oracle}`).

## Layout rules

- A new driver goes under the kind it is (`sdr/`, …); add a kind when the first device of it
  arrives. Crate names stay `rsl-<device>` (`rsl-usdr`), independent of the path.
- Code shared by several drivers of one kind (an SDR tune/rate/stream abstraction, say)
  lives in `devices/<kind>/` once a second driver needs it, not before.
- Transports any device can use (raw sockets, USB) are not devices: they live at the
  workspace root (`rawsock/`, …), and drivers depend on them.
- FFI crates wrapping a vendor's C library stay excluded at the workspace root (`usdr/`).
