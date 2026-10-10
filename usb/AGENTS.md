# rsl-usb

> Inherits the workspace-root `../AGENTS.md`. `CLAUDE.md` is a symlink to this file.

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
- Out of scope for v0: isochronous, hotplug, async, descriptor parsing beyond what sysfs
  lists, zero-length-packet flags.

## Unsafe

All of it is in `usbfs.rs` (ioctls, mmap), `buffer.rs` (the mapping), `queue.rs` and
`dispatch.rs` (in-flight transfer ownership). Rules:
- A submitted `Transfer` is touched only by the kernel until reaped; its `Urb` is its first
  field, and its buffer pointer is taken after it is boxed (Miri caught the earlier order).
- The kernel writes a transfer's outcome and IN data during the reap ioctl, so nothing is
  freed before it is reaped.
- The request codes and structure layouts are checked against `<linux/usbdevice_fs.h>`
  (const size asserts; request-code test on x86-64).

## Verify

```sh
cargo clippy -p rsl-usb --all-targets -- -D warnings && cargo test -p rsl-usb
cargo +nightly miri test -p rsl-usb --lib    # queue/dispatch ownership, against the fake backend
RSL_USB_TEST_DEVICE=vvvv:pppp cargo test -p rsl-usb -- --ignored   # real device, detaches its driver
```
