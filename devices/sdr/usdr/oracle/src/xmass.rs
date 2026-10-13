//! A fake XMASS device, the `usdr_pcie_uram` driver's `/dev/xmassN`, and libusdr's switch
//! firmware tool (`tools/xmass_fwupd.c`) run against it ([`xmass_fwupd`]).
//!
//! The tool reaches the device only through `open`, `ioctl` and `close`, which
//! `shim/xmass_fwupd.c` redirects here; rsl-usdr's port reaches the same [`FakeXmass`]
//! through its `SwitchFlash` trait. Both drivers' requests land in one list, so they compare
//! request for request.
//!
//! The fake models the driver's interface as the tool (MIT) and the interface header use
//! it, plus behaviour observed of the driver (GPL-2.0, nothing taken from it; see
//! PORTING.md's provenance note): the IDs are ten bytes, the flash 64 KiB, an erase blanks
//! it, a write programs it (bits only cleared, as NOR flash does) and may change bytes in
//! `0xe00..0xf00`, a read returns 1, the others 0. A request can be made to fail with an
//! errno, or to return a positive failure code without acting; a read that does so hands
//! back the last image written, as a stale staging buffer would. The fake stands in for
//! the changed bytes by inverting the image's: the tool skips that range, so their content
//! does not matter, only that they differ.

use std::ffi::{CStr, CString, c_char, c_int, c_ulong, c_void};
use std::fmt;
use std::iter;
use std::panic::{self, AssertUnwindSafe};
use std::ptr;
use std::slice;
use std::sync::{Mutex, PoisonError};

use crate::{PANIC, SESSION, pcie_abi, resume_callback_panic};

/// The bytes the flash requests move (`BUFFER_SIZE` in the tool).
pub const FLASH_BYTES: usize = 65_536;
/// The bytes the IDs request writes (`info[10]` in the tool).
pub const IDS_BYTES: usize = 10;
/// Bytes the driver may change when it programs the flash, which the tool does not compare.
const SIGNATURE: std::ops::Range<usize> = 0xe00..0xf00;

/// `EINVAL`: the driver's answer to a request code it does not know.
const EINVAL: c_int = 22;
/// `EBADF`: an fd the fake did not hand out.
const EBADF: c_int = 9;
/// `ENOENT`: a path other than the fake's node.
const ENOENT: c_int = 2;
/// `EBUSY`: the driver's answer to a second open.
const EBUSY: c_int = 16;
/// `EIO`: the Rust side panicked; the panic is resumed once the tool returns.
const EIO: c_int = 5;
/// The fd the fake hands out.
const FAKE_FD: c_int = 1000;

/// One request the device received.
#[derive(Clone, PartialEq, Eq)]
pub enum Request {
    /// `PCIE_GETIDS`.
    Ids,
    /// `PCIE_FLASH_ERASE`.
    Erase,
    /// `PCIE_FLASH_WRITE`, with the image it carried.
    Write(Box<[u8; FLASH_BYTES]>),
    /// `PCIE_FLASH_READ`.
    Read,
    /// A request code the driver does not know.
    Unknown(u64),
}

/// The image as a length and an FNV-1a hash: a mismatch shows which request differs, not
/// 64 KiB of bytes.
impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ids => f.write_str("Ids"),
            Self::Erase => f.write_str("Erase"),
            Self::Write(image) => {
                let hash = image.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, &byte| {
                    (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
                });
                write!(f, "Write({FLASH_BYTES} bytes, FNV-1a {hash:#018x})")
            }
            Self::Read => f.write_str("Read"),
            Self::Unknown(code) => write!(f, "Unknown({code:#x})"),
        }
    }
}

/// The fake device: its IDs, its flash, the requests it received, and the failures it was
/// told to make.
#[derive(Clone, Debug)]
pub struct FakeXmass {
    /// What `PCIE_GETIDS` returns.
    ids: [u8; IDS_BYTES],
    /// The flash's first 64 KiB.
    flash: Box<[u8; FLASH_BYTES]>,
    /// The request, counted from 0, that fails instead of acting, and how.
    failure: Option<(usize, Failure)>,
    /// The last image written: what a read that fails without an errno hands back.
    staged: Option<Box<[u8; FLASH_BYTES]>>,
    /// A byte that reads back with its low bit flipped.
    misread: Option<usize>,
    /// Every request so far, the failed one included.
    requests: Vec<Request>,
}

impl FakeXmass {
    /// A device answering `ids` whose flash holds `flash`.
    #[must_use]
    pub fn new(ids: [u8; IDS_BYTES], flash: &[u8; FLASH_BYTES]) -> Self {
        Self {
            ids,
            flash: Box::new(*flash),
            failure: None,
            staged: None,
            misread: None,
            requests: Vec::new(),
        }
    }

    /// Fails request `index` (from 0) with `errno`, doing nothing else.
    #[must_use]
    pub fn failing(self, index: usize, errno: i32) -> Self {
        Self {
            failure: Some((index, Failure::Errno(errno))),
            ..self
        }
    }

    /// Makes request `index` (from 0) return the positive failure code `code` without
    /// acting.
    #[must_use]
    pub fn returning(self, index: usize, code: i32) -> Self {
        Self {
            failure: Some((index, Failure::Returns(code))),
            ..self
        }
    }

    /// Reads byte `at` back with its low bit flipped.
    #[must_use]
    pub fn misreading(self, at: usize) -> Self {
        Self {
            misread: Some(at),
            ..self
        }
    }

    /// The requests received, in order.
    #[must_use]
    pub fn requests(&self) -> &[Request] {
        &self.requests
    }

    /// The flash now.
    #[must_use]
    pub fn flash(&self) -> &[u8; FLASH_BYTES] {
        &self.flash
    }

    /// Records `request`: `Ok(None)` to act on it, `Ok(Some(code))` to return a failure
    /// code without acting, `Err(errno)` to fail.
    fn receive(&mut self, request: Request) -> Result<Option<i32>, c_int> {
        let index = self.requests.len();
        self.requests.push(request);
        match self.failure {
            Some((at, Failure::Errno(errno))) if at == index => Err(errno),
            Some((at, Failure::Returns(code))) if at == index => Ok(Some(code)),
            _ => Ok(None),
        }
    }

    /// `PCIE_GETIDS`: returns 0.
    ///
    /// # Errors
    ///
    /// The errno it was told to fail with.
    pub fn read_ids(&mut self, out: &mut [u8; IDS_BYTES]) -> Result<i32, i32> {
        if let Some(code) = self.receive(Request::Ids)? {
            return Ok(code);
        }
        *out = self.ids;
        Ok(0)
    }

    /// `PCIE_FLASH_ERASE`: returns 0.
    ///
    /// # Errors
    ///
    /// As [`Self::read_ids`].
    pub fn erase(&mut self) -> Result<i32, i32> {
        if let Some(code) = self.receive(Request::Erase)? {
            return Ok(code);
        }
        self.flash.fill(0xff);
        Ok(0)
    }

    /// `PCIE_FLASH_WRITE`: returns 0.
    ///
    /// # Errors
    ///
    /// As [`Self::read_ids`].
    pub fn write(&mut self, image: &[u8; FLASH_BYTES]) -> Result<i32, i32> {
        self.staged = Some(Box::new(*image));
        if let Some(code) = self.receive(Request::Write(Box::new(*image)))? {
            return Ok(code);
        }
        for (at, (cell, &byte)) in self.flash.iter_mut().zip(image.iter()).enumerate() {
            let byte = if SIGNATURE.contains(&at) { !byte } else { byte };
            *cell &= byte;
        }
        Ok(0)
    }

    /// `PCIE_FLASH_READ`: returns 1.
    ///
    /// # Errors
    ///
    /// As [`Self::read_ids`].
    pub fn read(&mut self, out: &mut [u8; FLASH_BYTES]) -> Result<i32, i32> {
        if let Some(code) = self.receive(Request::Read)? {
            if let Some(staged) = &self.staged {
                *out = **staged;
            }
            return Ok(code);
        }
        *out = *self.flash;
        if let Some(at) = self.misread {
            out[at] ^= 1;
        }
        Ok(1)
    }
}

/// How a request the fake was told to fail fails.
#[derive(Clone, Copy, Debug)]
enum Failure {
    /// With this errno.
    Errno(c_int),
    /// By returning this positive code without acting.
    Returns(c_int),
}

/// The installed device, with the node it answers at and whether it is open.
struct Node {
    /// The device.
    device: FakeXmass,
    /// Its path.
    path: CString,
    /// How the tool opened it: path and flags, in order.
    opens: Vec<(String, c_int)>,
    /// Whether it is open now.
    open: bool,
    /// How many times the tool closed it.
    closes: usize,
}

/// The device the shim forwards to while [`xmass_fwupd`] runs.
static NODE: Mutex<Option<Node>> = Mutex::new(None);

/// Runs `f` on the installed node, converting a panic into `-EIO` so it never unwinds
/// through C.
fn with_node(f: impl FnOnce(&mut Node) -> c_int) -> c_int {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let mut node = NODE.lock().unwrap_or_else(PoisonError::into_inner);
        f(node
            .as_mut()
            .expect("invariant: the tool calls back only while it runs"))
    }));
    result.unwrap_or_else(|payload| {
        PANIC
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_or_insert(payload);
        -EIO
    })
}

/// The shim's `open`: the fake's node, once at a time, as the driver allows.
///
/// # Safety
///
/// `path` must be a NUL-terminated string.
#[unsafe(no_mangle)]
unsafe extern "C" fn rsl_oracle_xmass_open(path: *const c_char, flags: c_int) -> c_int {
    // SAFETY: the caller guarantees a NUL-terminated string.
    let path = unsafe { CStr::from_ptr(path) };
    with_node(|node| {
        node.opens
            .push((path.to_string_lossy().into_owned(), flags));
        if path != node.path.as_c_str() {
            return -ENOENT;
        }
        if node.open {
            return -EBUSY;
        }
        node.open = true;
        FAKE_FD
    })
}

/// The shim's `close`.
#[unsafe(no_mangle)]
extern "C" fn rsl_oracle_xmass_close(fd: c_int) -> c_int {
    with_node(|node| {
        if fd != FAKE_FD || !node.open {
            return -EBADF;
        }
        node.open = false;
        node.closes += 1;
        0
    })
}

/// The shim's `ioctl`: decodes the request with the header's codes and hands it to the
/// device.
///
/// # Safety
///
/// For `PCIE_GETIDS`, `arg` must be valid for [`IDS_BYTES`] writable bytes; for
/// `PCIE_FLASH_WRITE` for [`FLASH_BYTES`] readable ones, for `PCIE_FLASH_READ` writable
/// ones: what the driver copies. The tool passes its `info[10]`, `buffer` and `rb_buffer`.
#[unsafe(no_mangle)]
unsafe extern "C" fn rsl_oracle_xmass_ioctl(
    fd: c_int,
    request: c_ulong,
    arg: *mut c_void,
) -> c_int {
    let code = |name: &str| {
        pcie_abi()
            .into_iter()
            .find_map(|(fact, value)| (fact == name).then_some(value))
            .expect("the header defines every XMASS request")
    };
    #[allow(
        clippy::useless_conversion,
        reason = "`c_ulong` is 32 bits on some targets"
    )]
    let request = u64::from(request);
    with_node(|node| {
        if fd != FAKE_FD || !node.open {
            return -EBADF;
        }
        let device = &mut node.device;
        let result = if request == code("PCIE_GETIDS") {
            // SAFETY: the caller guarantees `IDS_BYTES` writable bytes.
            let out = unsafe { &mut *arg.cast::<[u8; IDS_BYTES]>() };
            device.read_ids(out)
        } else if request == code("PCIE_FLASH_ERASE") {
            device.erase()
        } else if request == code("PCIE_FLASH_WRITE") {
            // SAFETY: the caller guarantees `FLASH_BYTES` readable bytes.
            let image = unsafe { &*arg.cast::<[u8; FLASH_BYTES]>() };
            device.write(image)
        } else if request == code("PCIE_FLASH_READ") {
            // SAFETY: the caller guarantees `FLASH_BYTES` writable bytes.
            let out = unsafe { &mut *arg.cast::<[u8; FLASH_BYTES]>() };
            device.read(out)
        } else {
            device.requests.push(Request::Unknown(request));
            Err(EINVAL)
        };
        result.unwrap_or_else(|errno| -errno)
    })
}

unsafe extern "C" {
    fn rsl_oracle_xmass_fwupd(
        argc: c_int,
        argv: *mut *mut c_char,
        log: *mut *mut c_char,
        log_len: *mut usize,
    ) -> c_int;
    fn free(ptr: *mut c_void);
}

/// The shim's `RSL_ORACLE_TOOL_SETUP_FAILED`: its output buffer could not be set up, so the
/// tool did not run.
const TOOL_SETUP_FAILED: c_int = -1;

/// What libusdr's switch firmware tool did.
#[derive(Debug)]
pub struct XmassRun {
    /// Its exit status.
    pub status: i32,
    /// What it printed, stdout and stderr interleaved.
    pub log: String,
    /// Each `open` it made: path and flags.
    pub opens: Vec<(String, i32)>,
    /// How many times it closed the device.
    pub closes: usize,
    /// The device afterwards, with the requests it received.
    pub device: FakeXmass,
}

/// Runs libusdr's `xmass_fwupd` with `args` after its program name, `device` answering at
/// `node`. File arguments are real paths.
///
/// Holds the oracle session while it runs: the tool's output replaces the process's
/// `stdout` and `stderr`.
///
/// # Panics
///
/// If an argument holds a NUL byte, or the shim cannot set up the tool's output buffer.
#[must_use]
pub fn xmass_fwupd(device: FakeXmass, node: &str, args: &[&str]) -> XmassRun {
    let strings: Vec<CString> = iter::once("xmass_fwupd")
        .chain(args.iter().copied())
        .map(|arg| CString::new(arg).expect("no NUL in tool arguments"))
        .collect();
    let mut pointers: Vec<*mut c_char> = strings
        .iter()
        .map(|arg| arg.as_ptr().cast_mut())
        .chain(iter::once(ptr::null_mut()))
        .collect();
    let count = c_int::try_from(strings.len()).expect("few arguments");
    let mut log: *mut c_char = ptr::null_mut();
    let mut log_len = 0;

    let _session = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
    *NODE.lock().unwrap_or_else(PoisonError::into_inner) = Some(Node {
        device,
        path: CString::new(node).expect("no NUL in the node's path"),
        opens: Vec::new(),
        open: false,
        closes: 0,
    });
    // SAFETY: `pointers` holds `count` NUL-terminated strings that outlive the call, then a
    // null; `log` and `log_len` are valid out-pointers.
    let status = unsafe {
        rsl_oracle_xmass_fwupd(count, pointers.as_mut_ptr(), &raw mut log, &raw mut log_len)
    };
    let node = NODE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
        .expect("invariant: installed above");
    resume_callback_panic();
    assert!(
        status != TOOL_SETUP_FAILED && !log.is_null(),
        "the shim could not set up the tool's output"
    );
    // SAFETY: `open_memstream` left `log_len` bytes at `log`.
    let text =
        String::from_utf8_lossy(unsafe { slice::from_raw_parts(log.cast(), log_len) }).into_owned();
    // SAFETY: `log` is `open_memstream`'s allocation, freed once, here.
    unsafe { free(log.cast()) };
    XmassRun {
        status,
        log: text,
        opens: node.opens,
        closes: node.closes,
        device: node.device,
    }
}
