//! libusdr's TX stream on the simulated XSDR: create, start, sync, send with its
//! statistics, the streams' options, stop and destroy, and the sim plugin's TX buffer
//! callbacks. As the uSDR oracle's (`devices/sdr/usdr/oracle/src/tx.rs`), for one board.

use std::ffi::{CStr, c_char, c_int, c_uint, c_void};
use std::ptr::{self, NonNull};
use std::slice;
use std::sync::PoisonError;

use xsdr_sim::SimBoard;

use super::{
    BOARD, DMS_START, DMS_STOP, EINVAL, ETIMEDOUT, Oracle, Stream, resume_callback_panic,
    usdr_dms_create_ex, usdr_dms_destroy, usdr_dms_op, usdr_dms_sync, with_board,
};

/// `EBUSY`: a second TX stream.
const EBUSY: c_int = 16;

unsafe extern "C" {
    fn xsdr_oracle_stream_option_set(stream: Stream, name: *const c_char, value: i64) -> c_int;
    fn usdr_dms_send_stat(
        stream: Stream,
        stream_buffs: *mut *const c_void,
        samples: c_uint,
        timestamp: u64,
        timeout: c_uint,
        stat: *mut SendStat,
    ) -> c_int;
}

/// libusdr's `usdr_dms_send_stat`: what the TX core reported with a send's buffer.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SendStat {
    /// The core's sample clock, low 32 bits (`lhwtime`).
    pub lhwtime: u64,
    /// Always 0 in libusdr (`opkttime`).
    pub opkttime: u64,
    /// The fourth status word (`ktime`).
    pub ktime: c_uint,
    /// Front-end and DMA drops (`underruns`).
    pub underruns: c_uint,
    /// FIFO fill (`fifo_used`).
    pub fifo_used: u8,
    /// Zeroed by libusdr when it fills the rest; the oracle presets it to tell a filled
    /// record from one libusdr left alone.
    reserved: [u8; 3],
}

impl SendStat {
    /// A record libusdr has not written: `reserved` is all ones.
    const UNWRITTEN: Self = Self {
        lhwtime: 0,
        opkttime: 0,
        ktime: 0,
        underruns: 0,
        fifo_used: 0,
        reserved: [0xff; 3],
    };
}

impl Oracle {
    /// Creates a TX stream on `/ll/stx/0` in `ci16` (`usdr_dms_create_ex`, no flags) on
    /// the channels in `channels`, libusdr's mask of channel numbers (1 for `a`, 2 for
    /// `b`, 3 for both), with `samples_per_packet` samples per packet.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned: `-EBUSY` if a TX stream exists.
    pub fn create_tx_stream(&mut self, channels: u64, samples_per_packet: u32) -> Result<(), i32> {
        let Some(dev) = self.dev else {
            return Err(-EINVAL);
        };
        if self.tx.is_some() {
            return Err(-EBUSY);
        }
        let mut stream: Stream = ptr::null_mut();
        // SAFETY: `dev` is open, the strings are NUL-terminated, `stream` is a valid
        // out-pointer.
        let errno = unsafe {
            usdr_dms_create_ex(
                dev.as_ptr(),
                c"/ll/stx/0".as_ptr(),
                c"ci16".as_ptr(),
                channels,
                samples_per_packet,
                0,
                &raw mut stream,
            )
        };
        resume_callback_panic();
        if errno != 0 {
            return Err(errno);
        }
        self.tx = NonNull::new(stream);
        Ok(())
    }

    /// Starts the TX stream (`usdr_dms_op(USDR_DMS_START)`).
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned; `-EINVAL` with no TX stream.
    pub fn start_tx_stream(&mut self) -> Result<(), i32> {
        self.tx_op(DMS_START)
    }

    /// Synchronises the TX stream alone with libusdr's `synctype`
    /// (`usdr_dms_sync(dev, synctype, 1, [tx])`).
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned; `-EINVAL` with no TX stream.
    pub fn sync_tx_stream(&mut self, synctype: &CStr) -> Result<(), i32> {
        let (Some(dev), Some(stream)) = (self.dev, self.tx) else {
            return Err(-EINVAL);
        };
        let mut streams = [stream.as_ptr()];
        // SAFETY: `dev` is open, the string is NUL-terminated, and `streams` holds one live
        // stream for the call.
        let errno =
            unsafe { usdr_dms_sync(dev.as_ptr(), synctype.as_ptr(), 1, streams.as_mut_ptr()) };
        resume_callback_panic();
        if errno == 0 { Ok(()) } else { Err(errno) }
    }

    /// Stops the TX stream (`usdr_dms_op(USDR_DMS_STOP)`).
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned; `-EINVAL` with no TX stream.
    pub fn stop_tx_stream(&mut self) -> Result<(), i32> {
        self.tx_op(DMS_STOP)
    }

    /// Destroys the TX stream (`usdr_dms_destroy`).
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned; `-EINVAL` with no TX stream.
    pub fn destroy_tx_stream(&mut self) -> Result<(), i32> {
        let Some(stream) = self.tx.take() else {
            return Err(-EINVAL);
        };
        // SAFETY: `stream` came from a successful create and is destroyed once.
        let errno = unsafe { usdr_dms_destroy(stream.as_ptr()) };
        resume_callback_panic();
        if errno == 0 { Ok(()) } else { Err(errno) }
    }

    /// Sends `samples` samples from one host buffer per channel of the stream, each its
    /// `ci16` bytes, at `timestamp` (`u64::MAX` for "now"), waiting up to `timeout_ms`
    /// (`usdr_dms_send_stat`), and returns the statistics libusdr wrote, or `None` when it
    /// wrote none.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned (`-ETIMEDOUT` while every sim buffer waits to
    /// play); `-EINVAL` with no TX stream.
    pub fn send_tx(
        &mut self,
        channels: &[&[u8]],
        samples: u32,
        timestamp: u64,
        timeout_ms: u32,
    ) -> Result<Option<SendStat>, i32> {
        let Some(stream) = self.tx else {
            return Err(-EINVAL);
        };
        let mut buffers: Vec<*const c_void> = channels
            .iter()
            .map(|buffer| buffer.as_ptr().cast::<c_void>())
            .collect();
        let mut stat = SendStat::UNWRITTEN;
        // SAFETY: `stream` is live, `buffers` holds one channel buffer per channel of the
        // stream, each the caller sized for `samples` in `ci16` (libusdr reads no more),
        // and `stat` is a valid out-pointer.
        let errno = unsafe {
            usdr_dms_send_stat(
                stream.as_ptr(),
                buffers.as_mut_ptr(),
                samples,
                timestamp,
                timeout_ms,
                &raw mut stat,
            )
        };
        resume_callback_panic();
        if errno != 0 {
            return Err(errno);
        }
        Ok((stat.reserved == [0; 3]).then_some(stat))
    }

    /// Runs `f` on the board while libusdr has it, such as to set the TX core's status.
    ///
    /// # Panics
    ///
    /// Only if the oracle's board went missing, which its session lock rules out.
    pub fn board<R>(&mut self, f: impl FnOnce(&mut SimBoard) -> R) -> R {
        f(BOARD
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
            .expect("invariant: a board is installed while an oracle is open"))
    }

    /// Sets the TX stream's `"mute"` option (`_sfetrx4_option_set`): bit 0 lane A, bit 1
    /// lane B.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned; `-EINVAL` with no TX stream.
    pub fn set_tx_mute(&mut self, mute: u8) -> Result<(), i32> {
        let stream = self.tx.ok_or(-EINVAL)?;
        Self::stream_option(stream, c"mute", mute.into())
    }

    /// Sets the TX stream's `"chmap"` option (`_sfetrx4_option_set`): `map` gives each
    /// channel of the stream its physical channel number; the rest of libusdr's 64-entry
    /// `channel_info_t` is unmapped (255), as `usdr_channel_info_map_default` leaves it.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned; `-EINVAL` with no TX stream or more than 64
    /// entries.
    pub fn set_tx_chmap(&mut self, map: &[u8]) -> Result<(), i32> {
        let stream = self.tx.ok_or(-EINVAL)?;
        let mut info = [u8::MAX; 64];
        info.get_mut(..map.len())
            .ok_or(-EINVAL)?
            .copy_from_slice(map);
        // The option reads the value as a `const channel_info_t *` during the call, while
        // `info` lives.
        Self::stream_option(stream, c"chmap", info.as_ptr() as i64)
    }

    /// Sets the RX stream's `"ready"` option (`usdr_dms_set_ready`).
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned; `-EINVAL` with no RX stream.
    pub fn set_rx_ready(&mut self) -> Result<(), i32> {
        let stream = self.stream.ok_or(-EINVAL)?;
        Self::stream_option(stream, c"ready", 1)
    }

    /// Sets a stream option on `stream`.
    fn stream_option(stream: NonNull<c_void>, name: &CStr, value: i64) -> Result<(), i32> {
        // SAFETY: `stream` is a live stream handle and `name` NUL-terminated; the options
        // set here take `value` as a number, or ("chmap") as a pointer its caller keeps
        // alive for the call.
        let errno = unsafe { xsdr_oracle_stream_option_set(stream.as_ptr(), name.as_ptr(), value) };
        resume_callback_panic();
        if errno == 0 { Ok(()) } else { Err(errno) }
    }

    /// Runs a stream operation on the TX stream.
    fn tx_op(&mut self, command: c_uint) -> Result<(), i32> {
        let Some(stream) = self.tx else {
            return Err(-EINVAL);
        };
        // SAFETY: `stream` is live; libusdr ignores the time for START and STOP.
        let errno = unsafe { usdr_dms_op(stream.as_ptr(), command, 0) };
        resume_callback_panic();
        if errno == 0 { Ok(()) } else { Err(errno) }
    }
}

/// The sim plugin's TX stream setup: the board's buffers, `block` bytes each.
#[unsafe(no_mangle)]
extern "C" fn xsdr_oracle_tx_open(block: usize) {
    with_board(|board| {
        board.tx_open(block);
        0
    });
}

/// The sim plugin's TX stream release.
#[unsafe(no_mangle)]
extern "C" fn xsdr_oracle_tx_close() {
    with_board(|board| {
        board.tx_close();
        0
    });
}

/// The sim plugin's TX buffer grant: fills `words` and sets `*has_record` when the buffer
/// comes with a completion record, or returns `-ETIMEDOUT` while every buffer waits.
///
/// # Safety
///
/// `words` must be valid for four writable `c_uint`s and `has_record` for one `c_int`.
#[unsafe(no_mangle)]
unsafe extern "C" fn xsdr_oracle_tx_grant(words: *mut c_uint, has_record: *mut c_int) -> c_int {
    // SAFETY: the caller guarantees both pointers' extents.
    let (words, has_record) = unsafe { (slice::from_raw_parts_mut(words, 4), &mut *has_record) };
    with_board(|board| {
        let Some(record) = board.tx_grant() else {
            return -ETIMEDOUT;
        };
        let record = record.record();
        *has_record = c_int::from(record.is_some());
        words.copy_from_slice(&record.unwrap_or_default());
        0
    })
}

/// The sim plugin's TX commit: the bytes libusdr converted into the granted buffer.
///
/// # Safety
///
/// `bytes` must be valid for `len` readable bytes.
#[unsafe(no_mangle)]
unsafe extern "C" fn xsdr_oracle_tx_load(bytes: *const c_void, len: usize) {
    // SAFETY: the caller guarantees the extent.
    let bytes = unsafe { slice::from_raw_parts(bytes.cast::<u8>(), len) };
    with_board(|board| {
        board.tx_load(bytes);
        0
    });
}
