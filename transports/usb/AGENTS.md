# usb

> Inherits the workspace-root `../../AGENTS.md`. `CLAUDE.md` is a symlink to this file.

Owned USB host access: enumerate, open, reset and claim devices; queue bulk and interrupt
transfers per endpoint; synchronous control transfers. Generic: nothing device-specific
belongs here (drivers live under `devices/`). Unpublished (`publish = false`).

## Shape (skeptic-reviewed, 2026-10-09)

- Portable public API with no OS concepts; only the Linux usbfs backend exists. macOS is the
  likely next backend (wait on the interface's mach port; fallback a private thread behind
  the same API), then Windows (overlapped WinUSB; `reset` may be `Unsupported`).
- `Interface` and the queues are owned, `Arc`-shared handles. A queue owns its in-flight
  transfers and buffers; `Drop` cancels and reaps them all, leaking (never freeing) any the
  kernel has not returned after a grace period.
- No internal thread: `dispatch.rs` gives one waiting thread at a time the reaper role, files
  each completion under its endpoint and wakes waiters, all under one lock.
- Bounded completion driving: `Interface::reap(NonZeroUsize)` files at most the supplied
  budget across the device; queues' `try_complete` only takes filed buffers. Both use
  `try_lock` and return `Busy` on contention. Exhausting the budget or `Busy` requires
  rescheduling before waiting for readiness, even if the next turn discovers an empty backend.
- Readiness (feature `mio`, skeptic-reviewed 2026-10-12): `Interface::readiness()` returns the
  device's one `Readiness`, a `mio::event::Source` over the usbfs node (WRITABLE only). It
  holds an `Interface` (so `reset` refuses), deregisters itself on Drop, and puts the
  dispatcher in driven mode: blocking waiters drain once without waiting, then wait for the
  reactor's filings. Cancellation needs no reactor turn because discard returns after the
  kernel completes the URB. No raw-fd (`AsFd`) access on public handles: it would bypass
  the one-driver guard.
- Cancellation and Drop remain blocking: Linux `USBDEVFS_DISCARDURB` calls `usb_kill_urb`.
  Caller-owned cleanup workers must own the whole queue through cancellation/reap/Drop;
  never pass borrowed pending pointers or join a blocked worker on a reactor. Kernel device
  locks can delay other ioctls too. Executor/worker supervision and public async driver
  handles remain unimplemented; readiness registration is untested against a real node.
- Out of scope for v0: isochronous, hotplug, managed async, descriptor parsing beyond what sysfs
  lists, zero-length-packet flags.

## Unsafe

All of it is in `usbfs.rs` (ioctls, mmap), `buffer.rs` (the mapping), `queue.rs` and
`dispatch.rs` (in-flight transfer ownership). Rules:
- A submitted `Transfer` is touched only by the kernel until reaped; its `Urb` is its first
  field, and its buffer pointer is taken after it is boxed (Miri caught the earlier order).
- The kernel writes a transfer's outcome and IN data during the reap ioctl, so nothing is
  freed before it is reaped.
- Discard accepts a submitted allocation still owned by its queue, including a reaped/filed
  completion racing cancellation. The address stays live until the owner takes it back;
  discard only looks up its identity. A refusal does not itself return buffer ownership.
- The request codes and structure layouts are checked against `<linux/usbdevice_fs.h>`
  (const size asserts; request-code test on x86-64).

## Verify

```sh
cargo clippy -p usb --all-targets -- -D warnings && cargo test -p usb
cargo +nightly miri test -p usb --lib    # queue/dispatch ownership, against the fake backend
cargo clippy -p usb --all-targets --features mio -- -D warnings && cargo test -p usb --features mio
RSL_USB_TEST_DEVICE=vvvv:pppp cargo test -p usb --features mio -- --ignored   # real device, detaches its driver
```
