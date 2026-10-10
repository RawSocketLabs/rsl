//! Linux usbfs: an open `/dev/bus/usb/BBB/DDD` node and the ioctls on it
//! (`<linux/usbdevice_fs.h>`). Sizes, offsets and request codes were checked against that
//! header, kernel 7.2, x86-64; the size asserts and the request-code test hold them there.

use std::ffi::c_void;
use std::os::fd::{AsFd, OwnedFd};
use std::path::Path;
use std::ptr::{self, NonNull};
use std::time::Duration;

use rustix::event::{PollFd, PollFlags, Timespec};
use rustix::fs::{Mode, OFlags};
use rustix::io::Errno;
use rustix::ioctl::{Ioctl, IoctlOutput, Opcode, opcode};
use rustix::mm::{MapFlags, ProtFlags};

use crate::buffer::Mapping;
use crate::dispatch::{Backend, Transfer};
use crate::error::Error;

/// `struct usbdevfs_ctrltransfer`: a synchronous control transfer.
#[repr(C)]
struct CtrlTransfer {
    request_type: u8,
    request: u8,
    value: u16,
    index: u16,
    length: u16,
    /// Milliseconds; 0 waits forever.
    timeout: u32,
    data: *mut c_void,
}

/// `struct usbdevfs_urb`: a queued transfer, as the kernel reads it at submission and writes
/// its outcome back when it is reaped.
#[repr(C)]
#[derive(Debug)]
pub(crate) struct Urb {
    /// `USBDEVFS_URB_TYPE_*`.
    pub(crate) kind: u8,
    /// The endpoint address, direction bit included.
    pub(crate) endpoint: u8,
    /// Zero, or a negated errno, once reaped.
    pub(crate) status: i32,
    flags: u32,
    /// The transfer's memory.
    pub(crate) buffer: *mut c_void,
    /// Its length.
    pub(crate) buffer_length: i32,
    /// Bytes transferred, once reaped.
    pub(crate) actual_length: i32,
    start_frame: i32,
    number_of_packets: i32,
    error_count: i32,
    signr: u32,
    usercontext: *mut c_void,
}

/// `struct usbdevfs_disconnect_claim`.
#[repr(C)]
struct DisconnectClaim {
    interface: u32,
    flags: u32,
    driver: [u8; 256],
}

const _: () = {
    assert!(size_of::<CtrlTransfer>() == 24);
    assert!(size_of::<Urb>() == 56);
    assert!(std::mem::offset_of!(Urb, buffer) == 16);
    assert!(std::mem::offset_of!(Urb, actual_length) == 28);
    assert!(std::mem::offset_of!(Urb, usercontext) == 48);
    assert!(size_of::<DisconnectClaim>() == 264);
};

/// The usbfs ioctl group.
const GROUP: u8 = b'U';
const CONTROL: Opcode = opcode::read_write::<CtrlTransfer>(GROUP, 0);
const SUBMITURB: Opcode = opcode::read::<Urb>(GROUP, 10);
const DISCARDURB: Opcode = opcode::none(GROUP, 11);
const REAPURBNDELAY: Opcode = opcode::write::<*mut c_void>(GROUP, 13);
const RELEASEINTERFACE: Opcode = opcode::read::<u32>(GROUP, 16);
const RESET: Opcode = opcode::none(GROUP, 20);
const GET_CAPABILITIES: Opcode = opcode::read::<u32>(GROUP, 26);
const DISCONNECT_CLAIM: Opcode = opcode::read::<DisconnectClaim>(GROUP, 27);

/// `USBDEVFS_URB_TYPE_INTERRUPT`.
pub(crate) const URB_INTERRUPT: u8 = 1;
/// `USBDEVFS_URB_TYPE_BULK`.
pub(crate) const URB_BULK: u8 = 3;
/// `USBDEVFS_DISCONNECT_CLAIM_EXCEPT_DRIVER`: detach any driver but this one.
const EXCEPT_DRIVER: u32 = 0x02;
/// `USBDEVFS_CAP_MMAP`: transfer buffers can be mapped from the device.
const CAP_MMAP: u32 = 0x20;

impl Urb {
    /// A transfer of `length` bytes on `endpoint`; `buffer` is set once the memory is in
    /// its final place.
    pub(crate) fn new(kind: u8, endpoint: u8, length: i32) -> Self {
        Self {
            kind,
            endpoint,
            status: 0,
            flags: 0,
            buffer: ptr::null_mut(),
            buffer_length: length,
            actual_length: 0,
            start_frame: 0,
            number_of_packets: 0,
            error_count: 0,
            signr: 0,
            usercontext: ptr::null_mut(),
        }
    }
}

/// An open usbfs device node.
#[derive(Debug)]
pub(crate) struct Usbfs {
    /// The node.
    fd: OwnedFd,

    /// The kernel can map transfer buffers from the device.
    can_map: bool,
}

impl Usbfs {
    /// Opens the node at `path` for reading and writing.
    pub(crate) fn open(path: &Path) -> Result<Self, Error> {
        let fd = rustix::fs::open(path, OFlags::RDWR | OFlags::CLOEXEC, Mode::empty()).map_err(
            |errno| match errno {
                Errno::ACCESS | Errno::PERM => Error::Access(path.to_owned()),
                Errno::NOENT => Error::Disconnected,
                other => other.into(),
            },
        )?;
        let mut capabilities = 0_u32;
        // SAFETY: GET_CAPABILITIES writes one `u32` through its argument.
        let can_map = unsafe { request(&fd, GET_CAPABILITIES, (&raw mut capabilities).cast()) }
            .is_ok_and(|_| capabilities & CAP_MMAP != 0);
        Ok(Self { fd, can_map })
    }

    /// Detaches any kernel driver from `interface` and claims it, in one step.
    pub(crate) fn claim(&self, interface: u8) -> Result<(), Error> {
        let mut claim = DisconnectClaim {
            interface: interface.into(),
            flags: EXCEPT_DRIVER,
            driver: [0; 256],
        };
        claim.driver[..6].copy_from_slice(b"usbfs\0");
        // SAFETY: DISCONNECT_CLAIM reads one `DisconnectClaim`, which outlives the call.
        unsafe { request(&self.fd, DISCONNECT_CLAIM, (&raw mut claim).cast()) }?;
        Ok(())
    }

    /// Releases a claimed interface.
    pub(crate) fn release(&self, interface: u8) {
        let mut number = u32::from(interface);
        // SAFETY: RELEASEINTERFACE reads one `u32`, which outlives the call.
        let _ = unsafe { request(&self.fd, RELEASEINTERFACE, (&raw mut number).cast()) };
    }

    /// Resets the device.
    pub(crate) fn reset(&self) -> Result<(), Errno> {
        // SAFETY: RESET takes no argument.
        unsafe { request(&self.fd, RESET, ptr::null_mut()) }.map(|_| ())
    }

    /// A synchronous control transfer of `data.len()` bytes; returns the bytes transferred.
    /// For an OUT request the kernel only reads `data`.
    pub(crate) fn control(
        &self,
        setup: [u8; 2],
        value: u16,
        index: u16,
        data: &mut [u8],
        timeout: Duration,
    ) -> Result<usize, Error> {
        let length = u16::try_from(data.len()).map_err(|_| Error::InvalidLength {
            len: data.len(),
            max_packet: usize::from(u16::MAX),
        })?;
        let mut transfer = CtrlTransfer {
            request_type: setup[0],
            request: setup[1],
            value,
            index,
            length,
            timeout: millis(timeout),
            data: data.as_mut_ptr().cast(),
        };
        // SAFETY: CONTROL reads the `CtrlTransfer` and moves at most `length` bytes through
        // `data`, which is borrowed mutably for the call.
        let done = unsafe { request(&self.fd, CONTROL, (&raw mut transfer).cast()) }?;
        Ok(usize::try_from(done).unwrap_or(0))
    }
}

impl Backend for Usbfs {
    unsafe fn submit(&self, transfer: NonNull<Transfer>) -> Result<(), Errno> {
        // SAFETY: the caller keeps the transfer alive and unaliased until it is reaped
        // (`Backend::submit`); the kernel reads its `Urb`, the first field, now, and writes
        // the outcome back when it is reaped.
        unsafe { request(&self.fd, SUBMITURB, transfer.as_ptr().cast()) }.map(|_| ())
    }

    unsafe fn discard(&self, transfer: NonNull<Transfer>) -> Result<(), Errno> {
        // SAFETY: DISCARDURB takes the submitted pointer as its argument and does not
        // dereference it in userspace.
        unsafe { request(&self.fd, DISCARDURB, transfer.as_ptr().cast()) }.map(|_| ())
    }

    fn reap(&self) -> Result<Option<NonNull<Transfer>>, Errno> {
        let mut reaped: *mut c_void = ptr::null_mut();
        // SAFETY: REAPURBNDELAY writes one pointer through its argument: a transfer this file
        // submitted. Before returning it the kernel writes the transfer's outcome into it,
        // which `Backend::submit` callers keep valid until now.
        match unsafe { request(&self.fd, REAPURBNDELAY, (&raw mut reaped).cast()) } {
            Ok(_) => Ok(NonNull::new(reaped.cast())),
            Err(Errno::AGAIN) => Ok(None),
            Err(errno) => Err(errno),
        }
    }

    fn wait_ready(&self, timeout: Duration) -> Result<(), Errno> {
        let mut fds = [PollFd::new(&self.fd, PollFlags::OUT)];
        let timeout = Timespec::try_from(timeout).unwrap_or(Timespec {
            tv_sec: i64::MAX,
            tv_nsec: 0,
        });
        match rustix::event::poll(&mut fds, Some(&timeout)) {
            Ok(_) | Err(Errno::INTR) => Ok(()),
            Err(errno) => Err(errno),
        }
    }

    fn map(&self, len: usize) -> Option<Mapping> {
        if !self.can_map || len == 0 {
            return None;
        }
        // SAFETY: a fresh shared mapping of the device at an address the kernel picks, so
        // nothing existing is replaced; usbfs backs it with transfer memory.
        let base = unsafe {
            rustix::mm::mmap(
                ptr::null_mut(),
                len,
                ProtFlags::READ | ProtFlags::WRITE,
                MapFlags::SHARED,
                self.fd.as_fd(),
                0,
            )
        }
        .ok()?;
        let base = NonNull::new(base.cast())?;
        // SAFETY: just mapped, `len` bytes, readable and writable, referenced nowhere else.
        Some(unsafe { Mapping::new(base, len) })
    }
}

/// `timeout` in whole milliseconds for the kernel, at least 1 so it never means "forever".
fn millis(timeout: Duration) -> u32 {
    u32::try_from(timeout.as_millis())
        .unwrap_or(u32::MAX)
        .max(1)
}

/// Runs one ioctl.
///
/// # Safety
///
/// `arg` must be what `opcode` expects: a pointer to its structure, valid for the reads and
/// writes the request makes, or null for a request without an argument.
unsafe fn request(fd: &OwnedFd, opcode: Opcode, arg: *mut c_void) -> Result<IoctlOutput, Errno> {
    // SAFETY: the caller pairs `opcode` with its argument.
    unsafe { rustix::ioctl::ioctl(fd, Request { opcode, arg }) }
}

/// An ioctl: an opcode and its argument.
struct Request {
    /// The request code.
    opcode: Opcode,

    /// A pointer to the request's structure, or null.
    arg: *mut c_void,
}

// SAFETY: `request`'s callers pair the opcode with its argument; the kernel may write through
// it, so the request is mutating; the output is the raw return value, borrowing nothing.
unsafe impl Ioctl for Request {
    type Output = IoctlOutput;

    const IS_MUTATING: bool = true;

    fn opcode(&self) -> Opcode {
        self.opcode
    }

    fn as_ptr(&mut self) -> *mut c_void {
        self.arg
    }

    unsafe fn output_from_ptr(
        out: IoctlOutput,
        _extract_output: *mut c_void,
    ) -> rustix::io::Result<Self::Output> {
        Ok(out)
    }
}

#[cfg(all(test, target_arch = "x86_64"))]
mod tests {
    use super::*;

    /// The request codes `<linux/usbdevice_fs.h>` defines on x86-64.
    #[test]
    fn request_codes_match_the_kernel_header() {
        assert_eq!(CONTROL, 0xc018_5500);
        assert_eq!(SUBMITURB, 0x8038_550a);
        assert_eq!(DISCARDURB, 0x550b);
        assert_eq!(REAPURBNDELAY, 0x4008_550d);
        assert_eq!(RELEASEINTERFACE, 0x8004_5510);
        assert_eq!(RESET, 0x5514);
        assert_eq!(GET_CAPABILITIES, 0x8004_551a);
        assert_eq!(DISCONNECT_CLAIM, 0x8108_551b);
    }
}
