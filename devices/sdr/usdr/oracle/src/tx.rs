//! libusdr's TX stream on the simulated board: create, start, send with its statistics,
//! the `"mute"` option, stop and destroy, and the sim plugin's TX buffer callbacks.

use std::ffi::{CStr, c_char, c_int, c_uint, c_void};
use std::ptr::{self, NonNull};
use std::slice;
use std::sync::PoisonError;

use rsl_usdr_sim::SimBoard;

use super::{
    BOARD, DMS_START, DMS_STOP, EBUSY, EINVAL, ETIMEDOUT, Oracle, Stream, resume_callback_panic,
    usdr_dms_create_ex, usdr_dms_destroy, usdr_dms_op, usdr_dms_sync, with_board,
};

unsafe extern "C" {
    fn usdr_dms_send_stat(
        stream: Stream,
        stream_buffs: *mut *const c_void,
        samples: c_uint,
        timestamp: u64,
        timeout: c_uint,
        stat: *mut SendStat,
    ) -> c_int;
    fn rsl_oracle_stream_option_set(stream: Stream, name: *const c_char, value: i64) -> c_int;
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
    /// Creates a TX stream on `/ll/stx/0`, channel 0, in libusdr's data format `dformat`
    /// (`"host@wire"`, or a host format alone), with `samples_per_packet` samples per
    /// packet.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned; `-EBUSY` if a TX stream exists.
    pub fn create_tx_stream_as(
        &mut self,
        dformat: &CStr,
        samples_per_packet: u32,
    ) -> Result<(), i32> {
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
                dformat.as_ptr(),
                1,
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

    /// Synchronises the streams that exist, RX then TX, with libusdr's `synctype`
    /// (`usdr_dms_sync(dev, synctype, n, [rx, tx])`).
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned; `-EINVAL` with no device.
    pub fn sync_streams(&mut self, synctype: &CStr) -> Result<(), i32> {
        let Some(dev) = self.dev else {
            return Err(-EINVAL);
        };
        let mut streams: Vec<Stream> = [self.stream, self.tx]
            .into_iter()
            .flatten()
            .map(NonNull::as_ptr)
            .collect();
        let count = c_uint::from(self.stream.is_some()) + c_uint::from(self.tx.is_some());
        // SAFETY: `dev` is open, the string is NUL-terminated, and `streams` holds live
        // streams for the call.
        let errno =
            unsafe { usdr_dms_sync(dev.as_ptr(), synctype.as_ptr(), count, streams.as_mut_ptr()) };
        resume_callback_panic();
        if errno == 0 { Ok(()) } else { Err(errno) }
    }

    /// Sends `samples` samples of host bytes `buffer` (one channel) at `timestamp`
    /// (`u64::MAX` for "now"), waiting up to `timeout_ms` (`usdr_dms_send_stat`), and
    /// returns the statistics libusdr wrote, or `None` when it wrote none.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned (`-ETIMEDOUT` while every sim buffer waits to
    /// play); `-EINVAL` with no TX stream.
    pub fn send_tx(
        &mut self,
        buffer: &[u8],
        samples: u32,
        timestamp: u64,
        timeout_ms: u32,
    ) -> Result<Option<SendStat>, i32> {
        let Some(stream) = self.tx else {
            return Err(-EINVAL);
        };
        let mut buffers = [buffer.as_ptr().cast::<c_void>()];
        let mut stat = SendStat::UNWRITTEN;
        // SAFETY: `stream` is live, `buffers` holds one channel buffer the caller sized for
        // `samples` in the stream's host format (libusdr reads no more), and `stat` is a
        // valid out-pointer.
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

    /// Sets the TX stream's `"mute"` option (`_sfetrx4_option_set`): bit 0 lane A, bit 1
    /// lane B.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned; `-EINVAL` with no TX stream.
    pub fn set_tx_mute(&mut self, mute: u8) -> Result<(), i32> {
        let Some(stream) = self.tx else {
            return Err(-EINVAL);
        };
        // SAFETY: `stream` is a live stream handle and the name is NUL-terminated.
        let errno =
            unsafe { rsl_oracle_stream_option_set(stream.as_ptr(), c"mute".as_ptr(), mute.into()) };
        resume_callback_panic();
        if errno == 0 { Ok(()) } else { Err(errno) }
    }

    /// Runs `f` on the board while libusdr has it, such as to set the TX core's status.
    ///
    /// # Panics
    ///
    /// Only if the oracle's board went missing, which its session lock rules out.
    pub fn board<R>(&mut self, f: impl FnOnce(&mut SimBoard) -> R) -> R {
        let mut board = BOARD.lock().unwrap_or_else(PoisonError::into_inner);
        f(board
            .as_mut()
            .expect("invariant: a board is installed while an oracle is open"))
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
extern "C" fn rsl_oracle_tx_open(block: usize) {
    with_board(|board| {
        board.tx_open(block);
        0
    });
}

/// The sim plugin's TX stream release.
#[unsafe(no_mangle)]
extern "C" fn rsl_oracle_tx_close() {
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
unsafe extern "C" fn rsl_oracle_tx_grant(words: *mut c_uint, has_record: *mut c_int) -> c_int {
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
unsafe extern "C" fn rsl_oracle_tx_load(bytes: *const c_void, len: usize) {
    // SAFETY: the caller guarantees the extent.
    let bytes = unsafe { slice::from_raw_parts(bytes.cast::<u8>(), len) };
    with_board(|board| {
        board.tx_load(bytes);
        0
    });
}
