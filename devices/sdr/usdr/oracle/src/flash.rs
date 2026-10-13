//! libusdr's flash driver (`ipblks/espi_flash.c`) on a simulated board, without the rest of
//! libusdr: the shim hands it a bare transport handle, so no board bring-up runs and the
//! board's trace holds the flash operations alone. [`usdr_flash`] runs libusdr's flash
//! update tool (`tools/usdr_flash.c`) the same way.

use std::ffi::{CStr, CString, c_char, c_int, c_uint, c_void};
use std::iter;
use std::ptr;
use std::slice;
use std::sync::{MutexGuard, PoisonError};

use rsl_usdr_sim::SimBoard;

use crate::{SESSION, install, resume_callback_panic, take_board};

/// `ESPI_FLASH_DONT_WRITE_HEADER`: leave the first 256 bytes unwritten.
pub const DONT_WRITE_HEADER: u32 = 1;
/// `ESPI_FLASH_DONT_ERASE`: write without erasing first.
pub const DONT_ERASE: u32 = 2;

/// Bytes past the caller's buffer libusdr's read and write touch: they move whole 32-bit
/// words, rounding the last partial one up (`espi_flash.c`:162, 349).
const WORD_SLACK: usize = 3;

unsafe extern "C" {
    fn rsl_oracle_flash_get_id(
        base: c_uint,
        id: *mut u32,
        out: *mut c_char,
        maxstr: usize,
    ) -> c_int;
    fn rsl_oracle_flash_read(
        base: c_uint,
        mmap_base: c_uint,
        offset: u32,
        size: u32,
        out: *mut u8,
    ) -> c_int;
    fn rsl_oracle_flash_erase(base: c_uint, size: u32, offset: u32) -> c_int;
    fn rsl_oracle_flash_write(
        base: c_uint,
        mmap_base: c_uint,
        data: *const u8,
        size: u32,
        offset: u32,
        flags: c_uint,
    ) -> c_int;
}

/// libusdr's `espi_flash_*` functions driving a simulated board's flash controller.
///
/// Holds the oracle session, so no [`Oracle`](crate::Oracle) can run alongside it.
pub struct FlashOracle {
    /// The controller's command/status register (`/ll/qspi_flash/base`).
    base: u32,
    /// The controller's transfer buffer (`cfg_mmap_base`).
    mmap_base: u32,
    /// Held for the oracle's lifetime; see [`SESSION`].
    _session: MutexGuard<'static, ()>,
}

/// What `espi_flash_get_id` returned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdReply {
    /// Its return value: 0, `-ENODATA` for a part it does not know, or a bus errno.
    pub errno: i32,
    /// The ID word.
    pub id: u32,
    /// The part's description.
    pub description: String,
}

impl FlashOracle {
    /// Installs `board` for libusdr's flash driver, which will address the controller at
    /// register `base` and its transfer buffer at `mmap_base`.
    #[must_use]
    pub fn new(board: SimBoard, base: u32, mmap_base: u32) -> Self {
        let session = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
        install(Some(board));
        Self {
            base,
            mmap_base,
            _session: session,
        }
    }

    /// `espi_flash_get_id`.
    pub fn get_id(&mut self) -> IdReply {
        let mut id = 0;
        let mut out = [0 as c_char; 128];
        // SAFETY: `id` is a valid out-pointer and `out` is valid for its 128 writable bytes,
        // the size passed.
        let errno =
            unsafe { rsl_oracle_flash_get_id(self.base, &raw mut id, out.as_mut_ptr(), out.len()) };
        resume_callback_panic();
        // SAFETY: libusdr NUL-terminates `out` (it clears it first, then `snprintf`s within
        // its size).
        let description = unsafe { CStr::from_ptr(out.as_ptr()) };
        IdReply {
            errno,
            id,
            description: description.to_string_lossy().into_owned(),
        }
    }

    /// `espi_flash_read` of `len` bytes from `offset`.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned.
    ///
    /// # Panics
    ///
    /// Only on a target whose `usize` is narrower than 32 bits.
    pub fn read(&mut self, offset: u32, len: u32) -> Result<Vec<u8>, i32> {
        let len_bytes = usize::try_from(len).expect("u32 fits usize");
        // libusdr writes whole words, up to three bytes past the length asked for.
        let mut out = vec![0; len_bytes + WORD_SLACK];
        // SAFETY: `out` is valid for `len` bytes plus the word slack libusdr writes past them.
        let errno = unsafe {
            rsl_oracle_flash_read(self.base, self.mmap_base, offset, len, out.as_mut_ptr())
        };
        resume_callback_panic();
        out.truncate(len_bytes);
        if errno == 0 { Ok(out) } else { Err(errno) }
    }

    /// `espi_flash_erase` of `len` bytes from `offset`; `(0, u32::MAX)` erases the chip.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned.
    pub fn erase(&mut self, offset: u32, len: u32) -> Result<(), i32> {
        // SAFETY: plain integers; the call reaches the board only through the shim.
        let errno = unsafe { rsl_oracle_flash_erase(self.base, len, offset) };
        resume_callback_panic();
        if errno == 0 { Ok(()) } else { Err(errno) }
    }

    /// `espi_flash_write` of `data` from `offset` with libusdr's `flags`
    /// ([`DONT_ERASE`], [`DONT_WRITE_HEADER`]).
    ///
    /// libusdr reads up to three bytes past `data` to fill its last word; they are `0xff`
    /// here, the padding `rsl-usdr` uses, so the two traces are comparable.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned.
    ///
    /// # Panics
    ///
    /// If `data` is 4 GiB or more, past libusdr's 32-bit length; or if it is shorter than
    /// the 256-byte header with both [`DONT_ERASE`] and [`DONT_WRITE_HEADER`], where libusdr
    /// underflows the length and reads far past `data` (`espi_flash.c`:340-344).
    pub fn write(&mut self, offset: u32, data: &[u8], flags: u32) -> Result<(), i32> {
        let size = u32::try_from(data.len()).expect("test data fits u32");
        assert!(
            flags & (DONT_ERASE | DONT_WRITE_HEADER) != DONT_ERASE | DONT_WRITE_HEADER
                || data.len() >= 256,
            "libusdr underflows a header-skipping write shorter than its header"
        );
        let mut padded = data.to_vec();
        padded.extend_from_slice(&[0xff; WORD_SLACK]);
        // SAFETY: `padded` is valid for `size` bytes plus the word slack libusdr reads past
        // them. A skipped header shortens what libusdr reads; the assert above rules out the
        // one case where it underflows `size` instead.
        let errno = unsafe {
            rsl_oracle_flash_write(
                self.base,
                self.mmap_base,
                padded.as_ptr(),
                size,
                offset,
                flags,
            )
        };
        resume_callback_panic();
        if errno == 0 { Ok(()) } else { Err(errno) }
    }

    /// Ends the session and returns the board.
    #[must_use]
    pub fn close(self) -> SimBoard {
        take_board()
    }
}

impl Drop for FlashOracle {
    fn drop(&mut self) {
        install(None);
    }
}

unsafe extern "C" {
    fn rsl_oracle_usdr_flash(
        argc: c_int,
        argv: *mut *mut c_char,
        reply: *const c_char,
        log: *mut *mut c_char,
        log_len: *mut usize,
    ) -> c_int;
    fn free(ptr: *mut c_void);
}

/// What libusdr's flash tool did to a board.
pub struct ToolRun {
    /// The tool's exit status: 0, or the code its failure path returns.
    pub status: i32,
    /// What it wrote to stderr: its messages and libusdr's log.
    pub log: String,
    /// The board, with the tool's operations in its trace.
    pub board: SimBoard,
}

/// Runs libusdr's flash tool (`tools/usdr_flash.c`) on `board`, with `args` after its
/// program name and `reply` typed at its prompt. Its device has the uSDR's description but
/// is neither brought up nor powered down (see `rsl_oracle_usdr_flash` in the shim), so
/// the trace holds the tool's own operations. File arguments are real paths.
///
/// Holds the oracle session while it runs.
///
/// # Panics
///
/// If an argument or the reply holds a NUL byte, or the shim cannot set up the tool's
/// streams.
#[must_use]
pub fn usdr_flash(board: SimBoard, args: &[&str], reply: &str) -> ToolRun {
    let strings: Vec<CString> = iter::once("usdr_flash")
        .chain(args.iter().copied())
        .map(|arg| CString::new(arg).expect("no NUL in tool arguments"))
        .collect();
    // getopt may permute the pointers, never the strings.
    let mut pointers: Vec<*mut c_char> = strings
        .iter()
        .map(|arg| arg.as_ptr().cast_mut())
        .chain(iter::once(ptr::null_mut()))
        .collect();
    let count = c_int::try_from(strings.len()).expect("few arguments");
    let reply = CString::new(reply).expect("no NUL in the reply");
    let mut log: *mut c_char = ptr::null_mut();
    let mut log_len = 0;

    let _session = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
    install(Some(board));
    // SAFETY: `pointers` holds `count` NUL-terminated strings that outlive the call, then a
    // null; `reply` is NUL-terminated; `log` and `log_len` are valid out-pointers.
    let status = unsafe {
        rsl_oracle_usdr_flash(
            count,
            pointers.as_mut_ptr(),
            reply.as_ptr(),
            &raw mut log,
            &raw mut log_len,
        )
    };
    let board = take_board();
    resume_callback_panic();
    assert!(
        !log.is_null(),
        "the shim could not set up the tool ({status})"
    );
    // SAFETY: `open_memstream` left `log_len` bytes at `log`.
    let text =
        String::from_utf8_lossy(unsafe { slice::from_raw_parts(log.cast(), log_len) }).into_owned();
    // SAFETY: `log` is `open_memstream`'s allocation, freed once, here.
    unsafe { free(log.cast()) };
    ToolRun {
        status,
        log: text,
        board,
    }
}
