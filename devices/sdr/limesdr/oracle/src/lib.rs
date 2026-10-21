//! Runs wavelet-lab libusdr's LimeSDR-mini (`u3_limesdr`) driver against a [`SimBoard`].
//!
//! libusdr (vendored once, in `../../../vendor/libusdr/`, pinned by its `PIN`) is compiled
//! with its hardware transports replaced by `shim/sim_plugin.c`. That plugin stands in for
//! the FT601 USB transport: it reports the `U3_LIMESDR` device ID, turns each low-level
//! operation into LMS64C frames with libusdr's own `lms64c_proto.c`, and hands every frame
//! to the board installed by [`Oracle::open`]; libusdr's `usleep` advances the board's
//! virtual clock. The board's trace is the reference the `u3_limesdr` port is checked against.
//! Its streams are a fake buffer boundary standing in for the FT601 stream pipes (not
//! vendored): one buffer per direction of the block size libusdr's `stream_limesdr.c` asks
//! for, filled from [`SimBoard::rx_block`] and committed to [`SimBoard::tx_block`], so
//! `stream_limesdr.c`'s own framing runs unchanged.
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
/// [`Oracle::create_rx_stream`] and [`Oracle::create_tx_stream`] drive). `USDR_LOGLEVEL` only
/// changes logging and is allowed. The FT601 transport's own variables, if any, are not in
/// the pinned source.
pub const FORBIDDEN_ENV: [&str; 2] = ["USDR_BARE_DEV", "USDR_DEBUG"];

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
    fn usdr_dms_sync(
        device: DmDev,
        synctype: *const c_char,
        scount: c_uint,
        pstream: *mut *mut c_void,
    ) -> c_int;
    fn usdr_dms_recv(
        stream: *mut c_void,
        stream_buffs: *mut *mut c_void,
        timeout_ms: c_uint,
        nfo: *mut RecvNfo,
    ) -> c_int;
    fn usdr_dms_send(
        stream: *mut c_void,
        stream_buffs: *mut *const c_void,
        samples: c_uint,
        timestamp: u64,
        timeout_ms: c_uint,
    ) -> c_int;
    fn usdr_dms_destroy(stream: *mut c_void) -> c_int;
    fn limesdr_shim_fpga_info(out: *mut u16) -> c_int;
    fn limesdr_shim_create_errno() -> c_int;
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

/// `usdr_dms_recv_nfo_t` without its flexible `parts`, which `stream_limesdr.c` never
/// writes.
#[repr(C)]
struct RecvNfo {
    fsymtime: u64,
    totsyms: c_uint,
    totlost: c_uint,
    max_parts: c_uint,
    extra: u64,
}

/// A stream libusdr created: its handle and packet size.
#[derive(Clone, Copy)]
struct Stream {
    handle: NonNull<c_void>,
    samples: usize,
}

/// One packet libusdr's `usdr_dms_recv` delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Received {
    /// `totsyms`: the samples written.
    pub samples: u32,
    /// `fsymtime`: the first sample's time.
    pub time: u64,
    /// `totlost`.
    pub lost: u32,
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
    /// The last RX stream created, until destroyed.
    rx: Option<Stream>,
    /// The last TX stream created, until destroyed.
    tx: Option<Stream>,
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
            rx: None,
            tx: None,
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

    /// Creates a `ci16` RX stream of `samples` per packet (`usdr_dms_create` on
    /// `/ll/srx/0`, channel 0): libusdr's stream initialisation runs
    /// `limesdr_prepare_streaming` (a default 1 MS/s rate if none is set, then the
    /// LMS7002M streamed up in both directions), then the transport's. A second create
    /// replaces the first as [`Self::receive`]'s stream; libusdr keeps both (and frees
    /// neither: `limesdr_device_unregister_stream` frees nothing).
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned.
    pub fn create_rx_stream(&mut self, samples: u32) -> Result<(), i32> {
        self.rx = Some(self.create_stream(c"/ll/srx/0", samples)?);
        Ok(())
    }

    /// Creates a `ci16` TX stream of `samples` per packet (`usdr_dms_create` on
    /// `/ll/stx/0`), as [`Self::create_rx_stream`].
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned.
    pub fn create_tx_stream(&mut self, samples: u32) -> Result<(), i32> {
        self.tx = Some(self.create_stream(c"/ll/stx/0", samples)?);
        Ok(())
    }

    /// `usdr_dms_create` of `object` in `ci16` on channel 0.
    fn create_stream(&mut self, object: &CStr, samples: u32) -> Result<Stream, i32> {
        let Some(dev) = self.dev else {
            return Err(-EINVAL);
        };
        let mut stream: *mut c_void = ptr::null_mut();
        // SAFETY: `dev` is open, the strings NUL-terminated and `stream` a valid
        // out-pointer, written only when a stream is created.
        let errno = unsafe {
            usdr_dms_create(
                dev.as_ptr(),
                object.as_ptr(),
                c"ci16".as_ptr(),
                1,
                samples,
                &raw mut stream,
            )
        };
        resume_callback_panic();
        if errno != 0 {
            return Err(errno);
        }
        let handle = NonNull::new(stream).ok_or(-EIO)?;
        let samples = usize::try_from(samples).expect("a u32 fits");
        Ok(Stream { handle, samples })
    }

    /// `usdr_dms_sync(dev, synctype, 0, NULL)`: `limesdr_device_stream_sync`, which starts
    /// the streams for any `synctype` but `"off"` and stops them for `"off"`.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned (it always returns 0).
    pub fn sync(&mut self, synctype: &CStr) -> Result<(), i32> {
        let Some(dev) = self.dev else {
            return Err(-EINVAL);
        };
        // SAFETY: `dev` is open and `synctype` NUL-terminated; no stream array is passed.
        let errno = unsafe { usdr_dms_sync(dev.as_ptr(), synctype.as_ptr(), 0, ptr::null_mut()) };
        resume_callback_panic();
        if errno == 0 { Ok(()) } else { Err(errno) }
    }

    /// `usdr_dms_recv` on the RX stream into `buffer`'s front, `ci16`.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned (`-ETIMEDOUT` when the board has no block).
    ///
    /// # Panics
    ///
    /// Without an RX stream, or when `buffer` is shorter than its packet: libusdr would
    /// write past it.
    pub fn receive(&mut self, buffer: &mut [[i16; 2]]) -> Result<Received, i32> {
        let stream = self.rx.expect("an RX stream");
        assert!(
            buffer.len() >= stream.samples,
            "a buffer for the whole packet"
        );
        let mut buffers = [buffer.as_mut_ptr().cast::<c_void>()];
        let mut nfo = RecvNfo {
            fsymtime: 0,
            totsyms: 0,
            totlost: 0,
            max_parts: 0,
            extra: 0,
        };
        // SAFETY: the stream is live: a destroyed stream is taken out of the oracle; its one
        // host buffer holds a packet of 4-byte `ci16` samples; `nfo` is valid for the fields
        // it writes.
        let errno = unsafe {
            usdr_dms_recv(
                stream.handle.as_ptr(),
                buffers.as_mut_ptr(),
                0,
                &raw mut nfo,
            )
        };
        resume_callback_panic();
        if errno != 0 {
            return Err(errno);
        }
        Ok(Received {
            samples: nfo.totsyms,
            time: nfo.fsymtime,
            lost: nfo.totlost,
        })
    }

    /// `usdr_dms_send` of `samples` (`ci16`) on the TX stream with `timestamp`, libusdr's
    /// `dm_time_t` (its top bit set means none).
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned.
    ///
    /// # Panics
    ///
    /// Without a TX stream.
    pub fn send(&mut self, samples: &[[i16; 2]], timestamp: u64) -> Result<(), i32> {
        let stream = self.tx.expect("a TX stream");
        let mut buffers = [samples.as_ptr().cast::<c_void>()];
        let count = c_uint::try_from(samples.len()).expect("a test-sized send");
        // SAFETY: the stream is live, as in `receive`; its one host buffer holds `count`
        // samples, read only.
        let errno = unsafe {
            usdr_dms_send(
                stream.handle.as_ptr(),
                buffers.as_mut_ptr(),
                count,
                timestamp,
                0,
            )
        };
        resume_callback_panic();
        if errno == 0 { Ok(()) } else { Err(errno) }
    }

    /// `usdr_dms_destroy` of the RX stream (`limesdr_device_unregister_stream`, which only
    /// forgets it).
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned.
    ///
    /// # Panics
    ///
    /// Without an RX stream.
    pub fn destroy_rx_stream(&mut self) -> Result<(), i32> {
        let stream = self.rx.take().expect("an RX stream");
        Self::destroy(stream)
    }

    /// `usdr_dms_destroy` of the TX stream, as [`Self::destroy_rx_stream`].
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned.
    ///
    /// # Panics
    ///
    /// Without a TX stream.
    pub fn destroy_tx_stream(&mut self) -> Result<(), i32> {
        let stream = self.tx.take().expect("a TX stream");
        Self::destroy(stream)
    }

    /// `usdr_dms_destroy`.
    fn destroy(stream: Stream) -> Result<(), i32> {
        // SAFETY: the stream is live and destroyed once (it is taken out of the oracle).
        let errno = unsafe { usdr_dms_destroy(stream.handle.as_ptr()) };
        resume_callback_panic();
        if errno == 0 { Ok(()) } else { Err(errno) }
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

/// The sim plugin's stream setup: stream 1 (`tx`) or 0 with blocks of `block_size`.
#[unsafe(no_mangle)]
extern "C" fn limesdr_oracle_stream_open(tx: c_uint, block_size: c_uint) -> c_int {
    with_board(|board| {
        board.stream_open(tx != 0, block_size);
        0
    })
}

/// `ETIMEDOUT`: the board had no RX block.
const ETIMEDOUT: c_int = 110;

/// The sim plugin's RX wait: copies the board's next block into `out`.
///
/// # Safety
///
/// `out` must be valid for `size` writable bytes.
#[unsafe(no_mangle)]
unsafe extern "C" fn limesdr_oracle_rx_block(out: *mut u8, size: c_uint) -> c_int {
    let size = usize::try_from(size).expect("a u32 fits");
    // SAFETY: the caller guarantees `out` is valid for `size` writable bytes.
    let out = unsafe { std::slice::from_raw_parts_mut(out, size) };
    with_board(|board| match board.rx_block() {
        None => -ETIMEDOUT,
        Some(block) => {
            assert_eq!(
                block.len(),
                out.len(),
                "the block the stream was set up for"
            );
            out.copy_from_slice(&block);
            0
        }
    })
}

/// The sim plugin's TX commit of `size` bytes at `data`, with the send's timestamp.
///
/// # Safety
///
/// `data` must be valid for `size` readable bytes.
#[unsafe(no_mangle)]
unsafe extern "C" fn limesdr_oracle_tx_block(data: *const u8, size: c_uint, time: u64) -> c_int {
    let size = usize::try_from(size).expect("a u32 fits");
    // SAFETY: the caller guarantees `data` is valid for `size` readable bytes.
    let data = unsafe { std::slice::from_raw_parts(data, size) };
    // libusdr's `dm_time_t` with its top bit set is no time.
    let time = (time < 1 << 63).then_some(time);
    with_board(|board| {
        if board.tx_block(data, time) {
            0
        } else {
            -EINVAL
        }
    })
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
