//! Runs wavelet-lab libusdr's LMS7 family (`m2_lm7_1`) driver against a [`SimBoard`].
//!
//! libusdr (vendored once, in `../../../vendor/libusdr/`, pinned by its `PIN`) is compiled
//! with its hardware transports replaced by `shim/sim_plugin.c`. That plugin reports the
//! `M2_LM7_1` device ID and forwards every low-level operation to the board installed by
//! [`Oracle::open`]; libusdr's `usleep` advances the board's virtual clock. The board's
//! trace is the reference the LMS7 family port is checked against.
//!
//! libusdr keeps process-global state, so one [`Oracle`] exists at a time: [`Oracle::open`]
//! blocks until any other instance is closed.

use std::any::Any;
use std::env;
use std::ffi::{CStr, c_char, c_int, c_uint, c_ulonglong, c_void};
use std::panic::{self, AssertUnwindSafe};
use std::ptr::{self, NonNull};
use std::slice;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;

use xsdr_sim::{I2cAddress, SimBoard};

/// `EINVAL`: an operation shape the board's transports reject.
const EINVAL: c_int = 22;
/// `EIO`: the Rust side panicked; the panic is resumed once libusdr returns.
const EIO: c_int = 5;
/// `EOPNOTSUPP`: an operation class the sim does not model (`USDR_LSOP_DRP`, `URAM`, ...).
/// None is used by open, power-up or close; seeing one fails the oracle loudly.
const EOPNOTSUPP: c_int = 95;

/// libusdr `ls_op` class `USDR_LSOP_HWREG`: FPGA registers (write, then read).
const LSOP_HWREG: c_uint = 0;
/// `USDR_LSOP_SPI`: one 32-bit SPI transaction.
const LSOP_SPI: c_uint = 1;
/// `USDR_LSOP_I2C_DEV`: I2C write-then-read.
const LSOP_I2C_DEV: c_uint = 2;

/// Environment variables the pinned libusdr reads on the paths the oracle drives. Each
/// changes the operation sequence, so none may be set while an oracle runs:
/// `USDR_BARE_DEV` (`_xsdr_init_revx` stops after the LMS reset pulse), `XSDR_FORCE`
/// (`xsdr_init` accepts unknown HWIDs), `USDR_TX_W`/`USDR_TX_H` (TX band table),
/// `LMS8_MPW2015`/`SSDR_LMS8B` (sSDR LMS8001 stepping), `USDR_FE_TYPE` (front-end probe
/// hint) and `USDR_DEBUG` (`_usdr_dmd_create` starts a debug socket service).
/// `USDR_LOGLEVEL` only changes logging and is allowed.
pub const FORBIDDEN_ENV: [&str; 8] = [
    "USDR_BARE_DEV",
    "XSDR_FORCE",
    "USDR_TX_W",
    "USDR_TX_H",
    "LMS8_MPW2015",
    "SSDR_LMS8B",
    "USDR_FE_TYPE",
    "USDR_DEBUG",
];

/// Serialises oracles: libusdr's device registry and logging are process-global.
static SESSION: Mutex<()> = Mutex::new(());
/// The board the C plugin forwards to while an oracle is open.
static BOARD: Mutex<Option<SimBoard>> = Mutex::new(None);
/// A panic raised inside a callback, held until control is back in Rust.
static PANIC: Mutex<Option<Box<dyn Any + Send>>> = Mutex::new(None);

/// Opaque libusdr device-manager handle (`pdm_dev_t`).
type DmDev = *mut c_void;

/// One call into libusdr's LMS7002M chip driver (`hw/lms7002m/lms7002m.c`), as
/// [`lms7002m_run`] makes it. Enum arguments take libusdr's codes; signed ones are passed
/// as their two's-complement bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lms7002mCall {
    /// `lms7002m_mac_set(mac)`: `LMS7_CH_*`, 0 to 3.
    MacSet(u32),
    /// `lms7002m_rfe_path(path, mode)`: `RFE_NONE`.. `RFE_LNAW`, `RFE_MODE_*`.
    RfePath(u32, u32),
    /// `lms7002m_trf_path(path, mode)`: `TRF_MUTE`.. `TRF_B2`, `TRF_MODE_*`.
    TrfPath(u32, u32),
    /// `lms7002m_afe_enable(rxa, rxb, txa, txb)`.
    AfeEnable([bool; 4]),
    /// `lms7002m_xxtsp_enable(tsp, enable)`: `LMS_TXTSP` 0, `LMS_RXTSP` 1.
    TspEnable(u32, bool),
    /// `lms7002m_destroy`.
    Destroy,
    /// `lms7002m_limelight_configure(conf)`, the configuration packed as the shim's
    /// `unpack_conf` reads it: flags, then the divider and delay bytes.
    LimelightConfigure(u32, u32),
    /// `lms7002m_limelight_reset`.
    LimelightReset,
    /// `lms7002m_limelight_fifo_reset(rx, tx)`.
    LimelightFifoReset(bool, bool),
    /// `lms7002m_limelight_map(siso1, siso2, map1, map2)`: SISO flags in bits 0 and 1,
    /// each map as four 2-bit slots, slot 0 lowest.
    LimelightMap(u32, u32, u32),
    /// `lms7002m_limelight_switch_rx_mode(conf)`, packed as for [`Self::LimelightConfigure`].
    LimelightSwitchRxMode(u32, u32),
    /// `lms7002m_xxtsp_int_dec(tsp, order)`.
    TspIntDec(u32, u32),
    /// `lms7002m_rxtsp_dc_corr(bypass, window)`.
    RxtspDcCorr(bool, u32),
    /// `lms7002m_xxtsp_cmix(tsp, freq)`.
    TspCmix(u32, i32),
    /// `lms7002m_xxtsp_bst(tsp)`.
    TspBist(u32),
    /// `lms7002m_xxtsp_reset(tsp)`.
    TspReset(u32),
    /// `lms7002m_xxtsp_bst_isdone(tsp, &done)`; `out[0]` is `done`.
    TspBistDone(u32),
    /// `lms7002m_rbb_path(path, mode)`: `RBB_*` paths and modes.
    RbbPath(u32, u32),
    /// `lms7002m_rbb_pga(gainx10)`.
    RbbPga(i32),
    /// `lms7002m_rbb_lpf_raw({ rcc, r, c })`.
    RbbLpfRaw(u32, u32, u32),
    /// `lms7002m_tbb_path(path, mode)`: `TBB_*` paths and modes.
    TbbPath(u32, u32),
    /// `lms7002m_tbb_lpf_raw({ rcc, r, c })`.
    TbbLpfRaw(u32, u32, u32),
    /// `lms7002m_tbb_gain(gain)`.
    TbbGain(i8),
    /// `lms7002m_rfe_gain(stage, gainx10, &gout)`; `out[0]` is `gout`.
    RfeGain(u32, i32),
    /// `lms7002m_trf_gain(stage, gainx10, &gout)`; `out[0]` is `gout` (0 when unset).
    TrfGain(u32, i32),
    /// `lms7002m_cgen_tune(fref, outfreq, txdiv_ord)`.
    CgenTune(u32, u32, u32),
    /// `lms7002m_cgen_disable`.
    CgenDisable,
    /// `lms7002m_sxx_tune(path, fref, lofreq, lochen)`: `SXX_RX` 0, `SXX_TX` 1.
    SxxTune(u32, u32, u32, bool),
    /// `lms7002m_sxx_disable(path)`.
    SxxDisable(u32),
    /// `lms7002m_rbb_lpf_def(bw, lpf_l, &p)`, which touches no register; `out` is
    /// `rcc`, `r`, `c`.
    RbbLpfDef(u32, bool),
    /// `lms7002m_tbb_lpf_def(bw, lpf_l, &p)`, likewise.
    TbbLpfDef(u32, bool),
    /// `lms7002m_create(.., lsaddr 0, ldo_mask, txrx_clk, ..)` on the same state.
    Create(u32, bool),
}

/// The shim's `struct lms7002m_op`.
#[repr(C)]
struct LmsOp {
    kind: c_uint,
    a: c_uint,
    b: c_uint,
    c: c_uint,
    d: c_uint,
    out: [c_int; 3],
}

impl From<Lms7002mCall> for LmsOp {
    fn from(call: Lms7002mCall) -> Self {
        let op = |kind, a, b, c, d| Self {
            kind,
            a,
            b,
            c,
            d,
            out: [0; 3],
        };
        let bits = |value: i32| u32::from_ne_bytes(value.to_ne_bytes());
        match call {
            Lms7002mCall::MacSet(mac) => op(0, mac, 0, 0, 0),
            Lms7002mCall::RfePath(path, mode) => op(1, path, mode, 0, 0),
            Lms7002mCall::TrfPath(path, mode) => op(2, path, mode, 0, 0),
            Lms7002mCall::AfeEnable([a, b, c, d]) => op(3, a.into(), b.into(), c.into(), d.into()),
            Lms7002mCall::TspEnable(tsp, enable) => op(4, tsp, enable.into(), 0, 0),
            Lms7002mCall::Destroy => op(5, 0, 0, 0, 0),
            Lms7002mCall::LimelightConfigure(flags, bytes) => op(6, flags, bytes, 0, 0),
            Lms7002mCall::LimelightReset => op(7, 0, 0, 0, 0),
            Lms7002mCall::LimelightFifoReset(rx, tx) => op(8, rx.into(), tx.into(), 0, 0),
            Lms7002mCall::LimelightMap(siso, map1, map2) => op(9, siso, map1, map2, 0),
            Lms7002mCall::LimelightSwitchRxMode(flags, bytes) => op(10, flags, bytes, 0, 0),
            Lms7002mCall::TspIntDec(tsp, order) => op(11, tsp, order, 0, 0),
            Lms7002mCall::RxtspDcCorr(bypass, window) => op(12, bypass.into(), window, 0, 0),
            Lms7002mCall::TspCmix(tsp, freq) => op(13, tsp, bits(freq), 0, 0),
            Lms7002mCall::TspBist(tsp) => op(14, tsp, 0, 0, 0),
            Lms7002mCall::TspReset(tsp) => op(15, tsp, 0, 0, 0),
            Lms7002mCall::TspBistDone(tsp) => op(16, tsp, 0, 0, 0),
            Lms7002mCall::RbbPath(path, mode) => op(17, path, mode, 0, 0),
            Lms7002mCall::RbbPga(gain) => op(18, bits(gain), 0, 0, 0),
            Lms7002mCall::RbbLpfRaw(rcc, r, c) => op(19, rcc, r, c, 0),
            Lms7002mCall::TbbPath(path, mode) => op(20, path, mode, 0, 0),
            Lms7002mCall::TbbLpfRaw(rcc, r, c) => op(21, rcc, r, c, 0),
            Lms7002mCall::TbbGain(gain) => op(22, bits(gain.into()), 0, 0, 0),
            Lms7002mCall::RfeGain(stage, gain) => op(23, stage, bits(gain), 0, 0),
            Lms7002mCall::TrfGain(stage, gain) => op(24, stage, bits(gain), 0, 0),
            Lms7002mCall::CgenTune(fref, out, order) => op(25, fref, out, order, 0),
            Lms7002mCall::CgenDisable => op(26, 0, 0, 0, 0),
            Lms7002mCall::SxxTune(path, fref, lo, lochen) => op(27, path, fref, lo, lochen.into()),
            Lms7002mCall::SxxDisable(path) => op(28, path, 0, 0, 0),
            Lms7002mCall::RbbLpfDef(bw, low) => op(29, bw, low.into(), 0, 0),
            Lms7002mCall::TbbLpfDef(bw, low) => op(30, bw, low.into(), 0, 0),
            Lms7002mCall::Create(ldo, txrx) => op(31, ldo, txrx.into(), 0, 0),
        }
    }
}

/// What one [`Lms7002mCall`] returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lms7002mResult {
    /// The call's result: 0 or a negative errno.
    pub status: i32,
    /// Values the call reports beside it (see [`Lms7002mCall`]); zero otherwise.
    pub out: [i32; 3],
}

/// Runs `calls` in order through libusdr's LMS7002M driver on a zeroed chip state (as
/// `xsdr_ctor` leaves it, SPI target 0), its SPI reaching `board`. Returns each call's
/// result and the board with their trace. Serialised with [`Oracle`]s, which share the
/// board slot.
///
/// # Panics
///
/// If a callback panicked (resumed here).
#[must_use]
pub fn lms7002m_run(board: SimBoard, calls: &[Lms7002mCall]) -> (Vec<Lms7002mResult>, SimBoard) {
    let _session = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
    install(Some(board));
    let mut ops: Vec<LmsOp> = calls.iter().copied().map(LmsOp::from).collect();
    let mut results = vec![0; ops.len()];
    let count = c_uint::try_from(ops.len()).expect("invariant: a short call list");
    // SAFETY: `ops` and `results` are valid for `count` elements each; the shim reads and
    // writes the former's `out` and writes the latter, and keeps neither.
    let status = unsafe { xsdr_oracle_lms7002m_run(ops.as_mut_ptr(), count, results.as_mut_ptr()) };
    let board = take_board();
    resume_callback_panic();
    assert_eq!(status, 0, "the shim knows every call kind");
    let results = results
        .into_iter()
        .zip(&ops)
        .map(|(status, op)| Lms7002mResult {
            status,
            out: op.out,
        })
        .collect();
    (results, board)
}

unsafe extern "C" {
    fn xsdr_oracle_lms7002m_run(ops: *mut LmsOp, count: c_uint, results: *mut c_int) -> c_int;
    fn usdr_dmd_create_string(connection_string: *const c_char, odev: *mut DmDev) -> c_int;
    fn usdr_dmd_close(dev: DmDev) -> c_int;
    fn usdr_dme_get_uint(dev: DmDev, path: *const c_char, oval: *mut u64) -> c_int;
    fn usdr_dme_set_uint(dev: DmDev, path: *const c_char, val: u64) -> c_int;
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
#[error("libusdr failed to open the board: errno {errno}")]
pub struct OpenError {
    /// The negative errno libusdr returned.
    pub errno: i32,
    /// The board, with the trace of everything libusdr did before failing.
    pub board: Box<SimBoard>,
}

impl Oracle {
    /// Opens `board` through libusdr's `usdr_dmd_create_string`: the device factory reads
    /// HWID, then `xsdr_init` identifies the variant and brings up its supplies, and (XSDR)
    /// the front-end probe runs. The RFIC stays unpowered; see [`Oracle::power_up`].
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
                "{name} must be unset: it changes libusdr's LMS7 family operation sequence"
            );
        }
        let session = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
        install(Some(board));

        let mut dev: DmDev = ptr::null_mut();
        // SAFETY: the connection string is NUL-terminated and `dev` is a valid out-pointer.
        let errno = unsafe { usdr_dmd_create_string(c"".as_ptr(), &raw mut dev) };

        let Some(dev) = NonNull::new(dev) else {
            resume_callback_panic();
            // A null handle with errno 0 would be a libusdr bug; still report a failure.
            let errno = if errno == 0 { -EIO } else { errno };
            return Err(OpenError {
                errno,
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
            board: Box::new(take_board()),
        })
    }

    /// Powers the RFIC up (`/dm/power/en` = 1, `xsdr_pwren`): LMS7002M supplies, reset
    /// release and `lms7002m_create`'s register reset and version check.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned.
    pub fn power_up(&mut self) -> Result<(), i32> {
        let Some(dev) = self.dev else {
            return Err(-EINVAL);
        };
        // SAFETY: `dev` is open and the path is NUL-terminated; its handler
        // (`dev_m2_lm7_1_pwren_set`) reads the value as a number.
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
    /// `path`'s handler must take `value` as a number: some (`/dm/rate/rxtxadcdac`, string
    /// paths) read it as a pointer, which this value is not. [`Self::set_rates`] covers
    /// `/dm/rate/rxtxadcdac`.
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

    /// Sets the RX and TX sample rates, with no converter floors
    /// (`/dm/rate/rxtxadcdac`, whose value is a pointer to four `u32`s).
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
        // (`dev_m2_lm7_1_rate_m_set`) reads four `u32`s through the pointer, which outlives
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

    /// Closes libusdr's device (`xsdr_dtor`: RFIC streaming down, LMS7002M destroy, power
    /// and LED off, TX MMCM power-down) and returns the board.
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
extern "C" fn xsdr_oracle_sleep_us(us: c_ulonglong) {
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

/// The sim plugin's `ls_op`: dispatches one libusdr low-level operation to the board.
///
/// `m2_lm7_1` installs a proxy in front of this (`usdr_device_m2_lm7_1_lsop`) that
/// handles TX-port `USDR_LSOP_DRP` itself as `REG_CFG_PHY_1` register traffic; any other
/// DRP reaches here and is refused.
///
/// # Safety
///
/// `pin` must be valid for `insz` writable bytes and `pout` for `outsz` readable bytes
/// (either may be null when its size is zero), as libusdr's `lowlevel_ops.ls_op` requires.
#[unsafe(no_mangle)]
unsafe extern "C" fn xsdr_oracle_ls_op(
    op: c_uint,
    addr: c_uint,
    insz: usize,
    pin: *mut c_void,
    outsz: usize,
    pout: *const c_void,
) -> c_int {
    let input: &mut [u8] = if insz == 0 {
        &mut []
    } else {
        // SAFETY: the caller guarantees `pin` is valid for `insz` writable bytes.
        unsafe { slice::from_raw_parts_mut(pin.cast(), insz) }
    };
    let output: &[u8] = if outsz == 0 {
        &[]
    } else {
        // SAFETY: the caller guarantees `pout` is valid for `outsz` readable bytes.
        unsafe { slice::from_raw_parts(pout.cast(), outsz) }
    };
    with_board(|board| match op {
        LSOP_HWREG => hwreg(board, addr, input, output),
        // SPI addresses carry config in [31:16]; the bus is [7:0] (`SPIEXT_LSOP_GET_BUS`).
        LSOP_SPI => spi(board, addr & 0xff, input, output),
        LSOP_I2C_DEV => i2c(board, I2cAddress::from_lsop(addr), input, output),
        _ => -EOPNOTSUPP,
    })
}

/// Register operation, as the `PCIe` transport performs it: each output word is written to
/// consecutive addresses from `addr`, then each input word is read the same way.
fn hwreg(board: &mut SimBoard, addr: u32, input: &mut [u8], output: &[u8]) -> c_int {
    if input.len() % 4 != 0 || output.len() % 4 != 0 {
        return -EINVAL;
    }
    for (reg, word) in (addr..).zip(output.chunks_exact(4)) {
        board.write_reg(
            reg,
            u32::from_le_bytes(word.try_into().expect("invariant: 4-byte chunk")),
        );
    }
    for (reg, word) in (addr..).zip(input.chunks_exact_mut(4)) {
        word.copy_from_slice(&board.read_reg(reg).to_le_bytes());
    }
    0
}

/// One 32-bit SPI word on bus `bus`; the readback is optional.
fn spi(board: &mut SimBoard, bus: u32, input: &mut [u8], output: &[u8]) -> c_int {
    let Ok(out) = <[u8; 4]>::try_from(output) else {
        return -EINVAL;
    };
    if !matches!(input.len(), 0 | 4) {
        return -EINVAL;
    }
    match board.spi(bus, u32::from_le_bytes(out)) {
        Ok(read) => {
            input.copy_from_slice(&read.to_le_bytes()[..input.len()]);
            0
        }
        // Both transports reject a bus beyond the device's SPI core count.
        Err(_) => -EINVAL,
    }
}

/// I2C write-then-read: at most three bytes written and four read, the limits both
/// transports enforce through `si2c_make_ctrl_reg` (`ipblks/si2c.c`).
///
/// The FPGA I2C core shifts received bytes into a 32-bit word, first byte most significant,
/// and both transports copy that word's little-endian bytes into `input`. Evidence (as in
/// the uSDR oracle): libusdr reads 16-bit chip values MSB first on the wire as a native
/// `u16`, here the DAC80501 `DEVID` it compares with 0x0194/0x0195/0x2195.
fn i2c(board: &mut SimBoard, addr: I2cAddress, input: &mut [u8], output: &[u8]) -> c_int {
    if output.len() > 3 || input.len() > 4 {
        return -EINVAL;
    }
    let read = board.i2c(addr, output, input.len());
    let word = read
        .iter()
        .fold(0u32, |word, &byte| word << 8 | u32::from(byte));
    let len = input.len();
    input.copy_from_slice(&word.to_le_bytes()[..len]);
    0
}

#[cfg(test)]
mod tests {
    use xsdr_sim::Variant;

    use super::*;

    /// The XTRX DAC80501.
    const DAC: I2cAddress = I2cAddress { bus: 0, addr: 0x4b };

    #[test]
    fn i2c_rejects_what_the_fpga_core_cannot_encode() {
        let mut board = SimBoard::new(Variant::Xtrx);
        assert_eq!(i2c(&mut board, DAC, &mut [0; 2], &[1, 0, 0, 0]), -EINVAL);
        assert_eq!(i2c(&mut board, DAC, &mut [0; 5], &[1]), -EINVAL);
        assert!(board.trace().is_empty(), "rejected before reaching the bus");
    }

    #[test]
    fn a_16_bit_i2c_readback_is_the_native_value() {
        let mut board = SimBoard::new(Variant::Xtrx);
        let mut id = [0; 2];
        assert_eq!(i2c(&mut board, DAC, &mut id, &[1]), 0);
        assert_eq!(u16::from_ne_bytes(id), 0x0195);
    }
}
