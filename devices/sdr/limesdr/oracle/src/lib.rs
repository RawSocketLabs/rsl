//! Runs wavelet-lab libusdr's LimeSDR-mini (`u3_limesdr`) driver against a [`SimBoard`].
//!
//! libusdr (vendored once, in `../../../vendor/libusdr/`, pinned by its `PIN`) is compiled
//! with its hardware transports replaced by `shim/sim_plugin.c`. That plugin stands in for
//! the FT601 USB transport: it reports the `U3_LIMESDR` device ID, turns each low-level
//! operation into LMS64C frames with libusdr's own `lms64c_proto.c`, and hands every frame
//! to the board installed by [`Oracle::open`]; libusdr's `usleep` advances the board's
//! virtual clock. The board's trace is the reference the `u3_limesdr` port is checked against.
//! The plugin refuses the transport's streams (`-EOPNOTSUPP`; the FT601 stream pipes are not
//! vendored), after libusdr's own stream initialisation has prepared the LMS7002M.
//!
//! libusdr keeps process-global state, so one [`Oracle`] exists at a time: [`Oracle::open`]
//! blocks until any other instance is closed.

use std::any::Any;
use std::env;
use std::ffi::{CStr, c_char, c_int, c_uint, c_ulonglong, c_void};
use std::panic::{self, AssertUnwindSafe};
use std::ptr::{self, NonNull};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;

use limesdr_sim::{DATA_LEN, FRAME_LEN, SimBoard};

/// `EINVAL`: an operation on an oracle that is already closed.
const EINVAL: c_int = 22;
/// `EIO`: the Rust side panicked; the panic is resumed once libusdr returns.
const EIO: c_int = 5;

/// Environment variables the pinned libusdr reads on the paths the oracle drives. Each
/// changes the operation sequence, so none may be set while an oracle runs:
/// `USDR_DEBUG` (`_usdr_dmd_create` starts a debug socket service) and `USDR_BARE_DEV`
/// (`u3_limesdr`'s stream initialisation refuses before preparing streaming, which
/// [`Oracle::create_rx_stream`] drives). `USDR_LOGLEVEL` only
/// changes logging and is allowed. The FT601 transport's own variables, if any, are not in
/// the pinned source.
pub const FORBIDDEN_ENV: [&str; 2] = ["USDR_BARE_DEV", "USDR_DEBUG"];

/// Samples per packet of [`Oracle::create_rx_stream`]'s stream: one 4 KiB burst of
/// `ci16` after its 16-byte header (`create_limesdr_stream`'s `burst_smpl`).
const RX_STREAM_SAMPLES: c_uint = (4096 - 16) / 4;

/// Serialises oracles: libusdr's device registry and logging are process-global.
static SESSION: Mutex<()> = Mutex::new(());
/// The board the C plugin forwards to while an oracle is open.
static BOARD: Mutex<Option<SimBoard>> = Mutex::new(None);
/// A panic raised inside a callback, held until control is back in Rust.
static PANIC: Mutex<Option<Box<dyn Any + Send>>> = Mutex::new(None);

/// Opaque libusdr device-manager handle (`pdm_dev_t`).
type DmDev = *mut c_void;

unsafe extern "C" {
    fn usdr_dmd_create_string(connection_string: *const c_char, odev: *mut DmDev) -> c_int;
    fn usdr_dmd_close(dev: DmDev) -> c_int;
    fn usdr_dme_get_uint(dev: DmDev, path: *const c_char, oval: *mut u64) -> c_int;
    fn usdr_dme_set_uint(dev: DmDev, path: *const c_char, val: u64) -> c_int;
    fn usdr_dms_create(
        device: DmDev,
        sobj: *const c_char,
        dformat: *const c_char,
        channels: u64,
        pktsyms: c_uint,
        outu: *mut *mut c_void,
    ) -> c_int;
    fn limesdr_shim_fpga_info(out: *mut u16) -> c_int;
    fn limesdr_shim_create_errno() -> c_int;
    fn limesdr_shim_stream_params(out: *mut c_uint) -> c_uint;
    fn get_transform_fn(
        from: *const c_char,
        to: *const c_char,
        inputs: c_uint,
        outputs: c_uint,
    ) -> TransformInfo;
    fn lms64c_fill_packet(
        cmd: u8,
        status: u8,
        id: u8,
        data: *const u8,
        length: c_uint,
        out: *mut LmsFrame,
        out_cnt: c_uint,
    ) -> c_int;
    fn lms64c_parse_packet(
        cmd: u8,
        input: *const LmsFrame,
        in_cnt: c_uint,
        data: *mut u8,
        length: c_uint,
    ) -> c_int;
}

/// The transport stream setup libusdr's `create_limesdr_stream` asks for
/// (`lowlevel_stream_params_t`'s fields it sets).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamParams {
    /// `streamno`: 0 for RX, 1 for TX (`LIMESDR_RX`, `LIMESDR_TX`).
    pub stream_number: u32,
    /// `block_size`: bytes per transport buffer, headers included.
    pub block_bytes: u32,
    /// `buffer_count`.
    pub buffers: u32,
    /// `flags`: `LLSF_NEED_FDPOLL` or 0.
    pub flags: u32,
}

/// libusdr's `transform_info_t`: the converter `get_transform_fn` selects and its size
/// function.
#[repr(C)]
struct TransformInfo {
    /// `cfunc`, null for no converter.
    convert: *const c_void,
    /// `sfunc`, null for no converter.
    size: Option<unsafe extern "C" fn(c_uint, bool) -> c_uint>,
}

/// libusdr's `get_transform_fn(from, to, 1, 1)` size function at `bytes` (`sfunc(bytes,
/// reverse)`): the converted size of `bytes` of `from`, or with `reverse` the `from`
/// size that converts to `bytes` of `to`. `None` where libusdr has no converter.
#[must_use]
pub fn transform_size(from: &CStr, to: &CStr, bytes: u32, reverse: bool) -> Option<u32> {
    // SAFETY: both names are NUL-terminated; the function only compares them.
    let info = unsafe { get_transform_fn(from.as_ptr(), to.as_ptr(), 1, 1) };
    let size = info.size?;
    if info.convert.is_null() {
        return None;
    }
    // SAFETY: a size function libusdr returned, called with plain values.
    Some(unsafe { size(bytes, reverse) })
}

/// A `proto_lms64c_t`, aligned as the 32- and 16-bit stores and loads libusdr's builder and
/// parser make into its payload (offset 8) require.
#[repr(C, align(4))]
struct LmsFrame([u8; FRAME_LEN]);

/// Most bytes [`lms64c_fill`] takes: two frames' worth, enough to show the one-frame limit.
const FILL_MAX: usize = 2 * DATA_LEN;

/// Most values [`lms64c_read_values`] parses: the 14 address-then-value records a reply's
/// payload holds; libusdr's parser reads past the payload for more.
pub const READ_VALUES_MAX: usize = DATA_LEN / 4;

/// libusdr's `lms64c_fill_packet` building at most one request frame for `cmd` from
/// `data` (as the C reads it: 32-bit host-order records for register writes, 16-bit
/// host-order addresses for register reads, bytes otherwise), status and peripheral 0.
/// `None` where it builds no frame (no data) or fails (data for more than one frame).
///
/// # Panics
///
/// When `data` is longer than two frames' payload, which no test needs.
#[must_use]
pub fn lms64c_fill(cmd: u8, data: &[u8]) -> Option<[u8; FRAME_LEN]> {
    assert!(data.len() <= FILL_MAX, "at most two frames' payload");
    let mut words = [0u32; FILL_MAX / 4];
    for (word, chunk) in words.iter_mut().zip(data.chunks(4)) {
        let mut bytes = [0; 4];
        bytes[..chunk.len()].copy_from_slice(chunk);
        *word = u32::from_ne_bytes(bytes);
    }
    let mut out = LmsFrame([0; FRAME_LEN]);
    let length = c_uint::try_from(data.len()).expect("invariant: bounded above");
    // SAFETY: `words` holds `data.len()` readable bytes, 4-aligned for the builder's 32-bit
    // loads; `out` is one writable, 4-aligned frame and `out_cnt` is 1.
    let built =
        unsafe { lms64c_fill_packet(cmd, 0, 0, words.as_ptr().cast(), length, &raw mut out, 1) };
    (built == 1).then_some(out.0)
}

/// libusdr's `lms64c_parse_packet` on one reply frame for register-read command `cmd`
/// (`CMD_LMS7002_RD`, `CMD_BRDSPI_RD`): the first `count` register values, as its callers
/// read them (host-order 16-bit words).
///
/// # Panics
///
/// When `count` exceeds [`READ_VALUES_MAX`], or libusdr reports a parse failure.
#[must_use]
pub fn lms64c_read_values(cmd: u8, reply: &[u8; FRAME_LEN], count: usize) -> Vec<u16> {
    assert!(
        count <= READ_VALUES_MAX,
        "the parser reads past the payload beyond"
    );
    let input = LmsFrame(*reply);
    let mut values = vec![0u16; count];
    let length = c_uint::try_from(2 * count).expect("invariant: bounded above");
    // SAFETY: `input` is one readable, 4-aligned frame (the parser loads 16-bit words from
    // its payload, at most 28 of them for 14 values, all inside it); `values` holds
    // `2 * count` writable bytes.
    let res = unsafe {
        lms64c_parse_packet(cmd, &raw const input, 1, values.as_mut_ptr().cast(), length)
    };
    assert_eq!(res, 0, "libusdr parsed the reply");
    values
}

/// libusdr opened on a simulated board.
pub struct Oracle {
    /// The open libusdr device; `None` once closed.
    dev: Option<NonNull<c_void>>,
    /// Held for the oracle's lifetime; see [`SESSION`].
    _session: MutexGuard<'static, ()>,
}

/// libusdr refused to open the board.
#[derive(Debug, thiserror::Error)]
#[error("libusdr failed to open the board: errno {errno} (device: {device_errno})")]
pub struct OpenError {
    /// The negative errno `usdr_dmd_create_string` returned. When the device's own init
    /// fails this is `-ENODEV`: `lowlevel_create` tries every transport plugin and returns
    /// the last one's result, and the `PCIe` plugin, tried after the FT601 one, finds nothing.
    pub errno: i32,
    /// The negative errno the `u3_limesdr` device's create and init returned, which
    /// libusdr's API masks (see [`OpenError::errno`]).
    pub device_errno: i32,
    /// The board, with the trace of everything libusdr did before failing.
    pub board: Box<SimBoard>,
}

impl Oracle {
    /// Opens `board` through libusdr's `usdr_dmd_create_string`: the `u3_limesdr` factory,
    /// then `limesdr_init` (reference-clock detection, LMS7002M reset, version check and
    /// `lms7002m_create`, streaming stopped).
    ///
    /// # Errors
    ///
    /// [`OpenError`] when libusdr's open fails.
    ///
    /// # Panics
    ///
    /// When a variable in [`FORBIDDEN_ENV`] is set: libusdr would take a different path
    /// than the one the trace stands for.
    pub fn open(board: SimBoard) -> Result<Self, OpenError> {
        for name in FORBIDDEN_ENV {
            assert!(
                env::var_os(name).is_none(),
                "{name} must be unset: it changes libusdr's LimeSDR operation sequence"
            );
        }
        let session = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
        install(Some(board));

        let mut dev: DmDev = ptr::null_mut();
        // SAFETY: the connection string is NUL-terminated and `dev` is a valid out-pointer.
        let errno = unsafe { usdr_dmd_create_string(c"".as_ptr(), &raw mut dev) };
        // SAFETY: a plain read of the shim's record of its last create.
        let device_errno = unsafe { limesdr_shim_create_errno() };

        let Some(dev) = NonNull::new(dev) else {
            resume_callback_panic();
            // A null handle with errno 0 would be a libusdr bug; still report a failure.
            let errno = if errno == 0 { -EIO } else { errno };
            return Err(OpenError {
                errno,
                device_errno,
                board: Box::new(take_board()),
            });
        };
        // Owned before any callback panic resumes, so unwinding closes the device.
        let mut oracle = Self {
            dev: Some(dev),
            _session: session,
        };
        resume_callback_panic();
        if errno == 0 {
            return Ok(oracle);
        }
        oracle.shutdown();
        Err(OpenError {
            errno,
            device_errno,
            board: Box::new(take_board()),
        })
    }

    /// Sets `/dm/power/en` = 1 (`dev_limesdr_pwren_set`, which does nothing on this
    /// family).
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned.
    pub fn power_up(&mut self) -> Result<(), i32> {
        let Some(dev) = self.dev else {
            return Err(-EINVAL);
        };
        // SAFETY: `dev` is open and the path is NUL-terminated; its handler reads the value
        // as a number.
        let errno = unsafe { usdr_dme_set_uint(dev.as_ptr(), c"/dm/power/en".as_ptr(), 1) };
        resume_callback_panic();
        if errno == 0 { Ok(()) } else { Err(errno) }
    }

    /// Sets a libusdr device-manager value, such as `c"/dm/rate/master"` or
    /// `c"/dm/sdr/0/rx/frequency"`.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned.
    ///
    /// # Safety
    ///
    /// `path`'s handler must take `value` as a number: `/dm/rate/rxtxadcdac` reads it as a
    /// pointer, which this value is not. [`Self::set_rates`] covers that path.
    pub unsafe fn set_uint(&mut self, path: &CStr, value: u64) -> Result<(), i32> {
        let Some(dev) = self.dev else {
            return Err(-EINVAL);
        };
        // SAFETY: `dev` is open and `path` NUL-terminated; the caller guarantees the
        // handler reads `value` as a number.
        let errno = unsafe { usdr_dme_set_uint(dev.as_ptr(), path.as_ptr(), value) };
        resume_callback_panic();
        if errno == 0 { Ok(()) } else { Err(errno) }
    }

    /// Sets the RX and TX sample rates with no converter floors (`/dm/rate/rxtxadcdac`,
    /// whose value is a pointer to four `u32`s).
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned.
    pub fn set_rates(&mut self, rx: u32, tx: u32) -> Result<(), i32> {
        let Some(dev) = self.dev else {
            return Err(-EINVAL);
        };
        let rates: [u32; 4] = [rx, tx, 0, 0];
        // SAFETY: `dev` is open and the path NUL-terminated; its handler
        // (`dev_limesdr_rate_m_set`) reads four `u32`s through the pointer, which outlives
        // the call.
        let errno = unsafe {
            usdr_dme_set_uint(
                dev.as_ptr(),
                c"/dm/rate/rxtxadcdac".as_ptr(),
                rates.as_ptr() as u64,
            )
        };
        resume_callback_panic();
        if errno == 0 { Ok(()) } else { Err(errno) }
    }

    /// Creates a `ci16` RX stream (`usdr_dms_create` on `/ll/srx/0`, channel 0): libusdr's
    /// stream initialisation runs `limesdr_prepare_streaming` (a default 1 MS/s rate if
    /// none is set, then the LMS7002M streamed up in both directions), then the
    /// transport's, which the shim refuses: the FT601 stream pipes are not vendored. So a
    /// stream is never created, and libusdr's success is `-EOPNOTSUPP`.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned.
    ///
    /// # Panics
    ///
    /// If libusdr created a stream, which the shim rules out.
    pub fn create_rx_stream(&mut self) -> Result<(), i32> {
        let Some(dev) = self.dev else {
            return Err(-EINVAL);
        };
        let mut stream: *mut c_void = ptr::null_mut();
        // SAFETY: `dev` is open, the strings NUL-terminated and `stream` a valid
        // out-pointer, written only when a stream is created.
        let errno = unsafe {
            usdr_dms_create(
                dev.as_ptr(),
                c"/ll/srx/0".as_ptr(),
                c"ci16".as_ptr(),
                1,
                RX_STREAM_SAMPLES,
                &raw mut stream,
            )
        };
        resume_callback_panic();
        assert!(
            stream.is_null(),
            "invariant: the shim refuses every transport stream"
        );
        if errno == 0 { Ok(()) } else { Err(errno) }
    }

    /// Creates a stream in libusdr's data format `format` (`host@wire`, such as
    /// `c"cf32@ci12"`), on `/ll/srx/0` if `rx`, else `/ll/stx/0`, with
    /// `samples_per_packet` samples per packet, as [`Self::create_rx_stream`] does: the
    /// shim refuses the transport's stream, so libusdr's success is `-EOPNOTSUPP`. Returns
    /// libusdr's errno and the transport stream setup `create_limesdr_stream` asked for,
    /// if it got that far.
    ///
    /// # Panics
    ///
    /// If libusdr created a stream, which the shim rules out, or asked for more than one
    /// transport stream.
    pub fn create_stream(
        &mut self,
        rx: bool,
        format: &CStr,
        samples_per_packet: u32,
    ) -> (i32, Option<StreamParams>) {
        let Some(dev) = self.dev else {
            return (-EINVAL, None);
        };
        let mut words = [0; 4];
        // SAFETY: `words` is valid for the four words the shim writes; this also clears
        // its count of requests.
        unsafe { limesdr_shim_stream_params(words.as_mut_ptr()) };
        let path = if rx { c"/ll/srx/0" } else { c"/ll/stx/0" };
        let mut stream: *mut c_void = ptr::null_mut();
        // SAFETY: `dev` is open, the strings NUL-terminated and `stream` a valid
        // out-pointer, written only when a stream is created.
        let errno = unsafe {
            usdr_dms_create(
                dev.as_ptr(),
                path.as_ptr(),
                format.as_ptr(),
                1,
                samples_per_packet,
                &raw mut stream,
            )
        };
        resume_callback_panic();
        assert!(
            stream.is_null(),
            "invariant: the shim refuses every transport stream"
        );
        // SAFETY: as above.
        let requests = unsafe { limesdr_shim_stream_params(words.as_mut_ptr()) };
        assert!(requests <= 1, "one transport stream per create");
        let [stream_number, block_bytes, buffers, flags] = words;
        let params = (requests == 1).then_some(StreamParams {
            stream_number,
            block_bytes,
            buffers,
            flags,
        });
        (errno, params)
    }

    /// Reads a libusdr device-manager value, such as `c"/dm/revision"`.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned.
    pub fn get_uint(&mut self, path: &CStr) -> Result<u64, i32> {
        let Some(dev) = self.dev else {
            return Err(-EINVAL);
        };
        let mut value = 0;
        // SAFETY: `dev` is open, `path` is NUL-terminated, `value` is a valid out-pointer.
        let errno = unsafe { usdr_dme_get_uint(dev.as_ptr(), path.as_ptr(), &raw mut value) };
        resume_callback_panic();
        if errno == 0 { Ok(value) } else { Err(errno) }
    }

    /// Reads gateware registers 0x0000-0x0003 with libusdr's own request builder
    /// (`lms64c_fill_get_fpga_info`) and reply parser. No vendored code calls the builder:
    /// this exercises the framing, not a libusdr sequence.
    ///
    /// # Errors
    ///
    /// The negative errno of the exchange: `-EIO` when the board refused it.
    pub fn fpga_info(&mut self) -> Result<[u16; 4], i32> {
        if self.dev.is_none() {
            return Err(-EINVAL);
        }
        let mut info = [0u16; 4];
        // SAFETY: `info` is valid for the four `u16`s the shim writes.
        let errno = unsafe { limesdr_shim_fpga_info(info.as_mut_ptr()) };
        resume_callback_panic();
        if errno == 0 { Ok(info) } else { Err(errno) }
    }

    /// How many operations the board has traced so far; slice the trace with it, after
    /// [`Oracle::close`], to isolate one call's operations.
    ///
    /// # Panics
    ///
    /// Only if the oracle's board went missing, which its session lock rules out.
    #[must_use]
    pub fn trace_len(&self) -> usize {
        BOARD
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .expect("invariant: a board is installed while an oracle is open")
            .trace()
            .len()
    }

    /// Closes libusdr's device (`limesdr_device_destroy`) and returns the board.
    #[must_use]
    pub fn close(mut self) -> SimBoard {
        self.shutdown();
        take_board()
    }

    /// Closes the libusdr device if it is still open, re-raising any callback panic.
    fn shutdown(&mut self) {
        self.close_device();
        resume_callback_panic();
    }

    /// Closes the libusdr device if it is still open.
    fn close_device(&mut self) {
        if let Some(dev) = self.dev.take() {
            // SAFETY: `dev` came from a successful `usdr_dmd_create_string` and is closed once.
            unsafe { usdr_dmd_close(dev.as_ptr()) };
        }
    }
}

impl Drop for Oracle {
    fn drop(&mut self) {
        self.close_device();
        if thread::panicking() {
            // Already unwinding: a second panic would abort and hide the first.
            PANIC.lock().unwrap_or_else(PoisonError::into_inner).take();
        } else {
            resume_callback_panic();
        }
        install(None);
    }
}

/// Replaces the board the C plugin forwards to.
fn install(board: Option<SimBoard>) {
    *BOARD.lock().unwrap_or_else(PoisonError::into_inner) = board;
}

/// Removes the installed board.
fn take_board() -> SimBoard {
    BOARD
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
        .expect("invariant: a board is installed while an oracle is open")
}

/// Re-raises a panic caught in a callback, now that no C frames are on the stack.
fn resume_callback_panic() {
    if let Some(payload) = PANIC.lock().unwrap_or_else(PoisonError::into_inner).take() {
        panic::resume_unwind(payload);
    }
}

/// Runs `f` on the installed board, converting a panic into `-EIO` so it never unwinds
/// through C.
fn with_board(f: impl FnOnce(&mut SimBoard) -> c_int) -> c_int {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let mut board = BOARD.lock().unwrap_or_else(PoisonError::into_inner);
        f(board
            .as_mut()
            .expect("invariant: libusdr calls back only while an oracle is open"))
    }));
    result.unwrap_or_else(|payload| {
        // Keep the first panic: later ones are usually libusdr's error path calling back.
        PANIC
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_or_insert(payload);
        -EIO
    })
}

/// libusdr's `usleep`, redirected by the shim. The redirect is process-wide, so a call
/// while no oracle is open returns at once.
#[unsafe(no_mangle)]
extern "C" fn limesdr_oracle_sleep_us(us: c_ulonglong) {
    if BOARD
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .is_none()
    {
        return;
    }
    with_board(|board| {
        board.sleep_us(us);
        0
    });
}

/// The sim plugin's frame exchange: hands one LMS64C request to the board and copies its
/// reply back.
///
/// # Safety
///
/// `request` must be valid for [`FRAME_LEN`] readable bytes and `reply` for [`FRAME_LEN`]
/// writable bytes; the shim passes two distinct `proto_lms64c_t`, which it checks are
/// that size.
#[unsafe(no_mangle)]
unsafe extern "C" fn limesdr_oracle_lms64c(request: *const u8, reply: *mut u8) -> c_int {
    // SAFETY: the caller guarantees `request` is valid for `FRAME_LEN` readable bytes; a
    // `[u8; N]` has alignment 1.
    let request = unsafe { &*request.cast::<[u8; FRAME_LEN]>() };
    with_board(|board| {
        let frame = board.lms64c(request);
        // SAFETY: the caller guarantees `reply` is valid for `FRAME_LEN` writable bytes and
        // does not alias `request`. `write` neither reads nor drops the (uninitialised)
        // destination; a `[u8; N]` has alignment 1.
        unsafe { reply.cast::<[u8; FRAME_LEN]>().write(frame) };
        0
    })
}
