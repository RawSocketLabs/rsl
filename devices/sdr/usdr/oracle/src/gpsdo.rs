//! libusdr's `usdr_dm_gpsdo` (`tools/usdr_dm_gpsdo.c`, compiled whole by `shim/gpsdo.c`):
//! the tool's own `main`, its control loop included, on a simulated board, reading a
//! scripted measurement word per poll and writing the DAC through libusdr. See the shim
//! for what is redirected.

use std::collections::VecDeque;
use std::ffi::{CString, c_char, c_int};
use std::ptr;
use std::sync::{Mutex, PoisonError};

use rsl_usdr_sim::SimBoard;

use super::{BOARD, SESSION, install, resume_callback_panic, take_board, with_board};

/// The words the tool's measurement reads load, in order.
static SCRIPT: Mutex<VecDeque<u32>> = Mutex::new(VecDeque::new());

/// `struct rsl_gpsdo_event` in the shim.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RawEvent {
    kind: i32,
    i0: i32,
    i1: i32,
    i2: i32,
    word: u64,
    a: f64,
    b: f64,
    c: f64,
}

unsafe extern "C" {
    fn rsl_oracle_gpsdo_run(
        argc: c_int,
        argv: *mut *mut c_char,
        events: *mut RawEvent,
        capacity: usize,
        count: *mut usize,
        dac_before: *mut u64,
    ) -> c_int;
}

/// A set-up call the tool made before its loop; the shim records it without performing it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Setup {
    /// `/dm/sdr/refclk/frequency`, in Hz.
    ReferenceFrequency(u64),
    /// `/dm/sdr/refclk/path`: whether it named `"external"`.
    ExternalReference(bool),
    /// `usdr_dmr_rate_set`, in samples per second.
    Rate(u64),
    /// `usdr_dms_create` of the RX stream, with its samples per packet.
    RxStream(u64),
    /// `usdr_dms_sync` `"any"` of this many streams (0 for another type).
    SyncAny(u64),
    /// `usdr_dms_op` with this command (`USDR_DMS_START` is 0).
    Op(u64),
}

/// One thing the tool did, in order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// A measurement read began, at this length of the board's trace.
    Read(usize),
    /// `"PPS=..."`: the word read, its pulse count, the measured frequency in Hz and the
    /// error in ppm.
    Pps {
        /// The word.
        word: u64,
        /// Its pulse count.
        count: i32,
        /// Its frequency in Hz.
        measured: f64,
        /// `error * 1e6`.
        error_ppm: f64,
    },
    /// `"Error more than 2%, skip iteration"`.
    Skip,
    /// `"POUT=..."`.
    Pout {
        /// The PID output.
        output: f64,
        /// The correction in Hz.
        correction_hz: f64,
        /// The DAC steps asked for, before their limit.
        steps: f64,
    },
    /// `"DAC value: ..."`, printed before a write.
    Dac {
        /// The new code.
        code: u32,
        /// The DAC's bits.
        bits: u32,
        /// The code it steps from.
        offset: u32,
        /// The limited step.
        step: f64,
    },
    /// The DAC write and libusdr's result.
    DacWrite {
        /// The code written.
        code: u64,
        /// libusdr's result (0 for success).
        result: i32,
    },
    /// `"No PPS trigger for more than ... seconds - entering holdover mode"`.
    Holdover(f64),
    /// `"Found PPS trigger"`.
    Found,
    /// A set-up call.
    Setup(Setup),
    /// A line on stderr.
    Stderr,
}

impl Event {
    /// Decodes the shim's record.
    ///
    /// # Panics
    ///
    /// On a kind the shim does not write (0: a line it did not recognise).
    fn decode(raw: &RawEvent) -> Self {
        match raw.kind {
            1 => Self::Read(usize::try_from(raw.word).expect("a trace length")),
            2 => Self::Pps {
                word: raw.word,
                count: raw.i0,
                measured: raw.a,
                error_ppm: raw.b,
            },
            3 => Self::Skip,
            4 => Self::Pout {
                output: raw.a,
                correction_hz: raw.b,
                steps: raw.c,
            },
            5 => Self::Dac {
                code: u32::from_ne_bytes(raw.i0.to_ne_bytes()),
                bits: u32::from_ne_bytes(raw.i1.to_ne_bytes()),
                offset: u32::from_ne_bytes(raw.i2.to_ne_bytes()),
                step: raw.a,
            },
            6 => Self::DacWrite {
                code: raw.word,
                result: raw.i0,
            },
            7 => Self::Holdover(raw.a),
            8 => Self::Found,
            9 => Self::Setup(match raw.i0 {
                1 => Setup::ReferenceFrequency(raw.word),
                2 => Setup::ExternalReference(raw.word != 0),
                3 => Setup::Rate(raw.word),
                4 => Setup::RxStream(raw.word),
                5 => Setup::SyncAny(raw.word),
                6 => Setup::Op(raw.word),
                other => panic!("unknown set-up call {other}"),
            }),
            10 => Self::Stderr,
            other => panic!("the tool printed a line the shim does not know (kind {other})"),
        }
    }
}

/// One run of the tool.
pub struct GpsdoRun {
    /// The tool's exit status.
    pub status: i32,
    /// What it did.
    pub events: Vec<Event>,
    /// The board after the tool closed it, with its trace.
    pub board: SimBoard,
    /// The code `set_dac_value`'s function `static` held when the run began: the code the
    /// tool last wrote in this process (nothing resets it), or 0. A first write of it is
    /// skipped.
    pub dac_before: u64,
}

/// Runs `usdr_dm_gpsdo` with `options` (without the program name) on `board`;
/// each measurement read loads the next of `words`, and one past them ends the loop.
///
/// # Panics
///
/// If an argument holds a NUL or the shim records a line it does not know.
#[must_use]
pub fn run_gpsdo(board: SimBoard, options: &[&str], words: &[u32]) -> GpsdoRun {
    let _session = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
    install(Some(board));
    *SCRIPT.lock().unwrap_or_else(PoisonError::into_inner) = words.iter().copied().collect();

    let strings: Vec<CString> = ["usdr_dm_gpsdo"]
        .iter()
        .chain(options)
        .map(|arg| CString::new(*arg).expect("no NUL in an argument"))
        .collect();
    let mut argv: Vec<*mut c_char> = strings.iter().map(|s| s.as_ptr().cast_mut()).collect();
    argv.push(ptr::null_mut());
    let count_c = c_int::try_from(strings.len()).expect("few arguments");
    // Two reads, a few lines and a write per word, the set-up and the end, with room.
    let capacity = 16 * words.len() + 64;
    let mut raw = vec![RawEvent::default(); capacity];
    let mut count = 0;
    let mut dac_before = 0;
    // SAFETY: `argv` holds `count_c` NUL-terminated strings that outlive the call, then a
    // null (getopt permutes the pointers, not the strings); `raw` has room for `capacity`
    // records and `count` and `dac_before` are valid out-pointers.
    let status = unsafe {
        rsl_oracle_gpsdo_run(
            count_c,
            argv.as_mut_ptr(),
            raw.as_mut_ptr(),
            capacity,
            &raw mut count,
            &raw mut dac_before,
        )
    };
    resume_callback_panic();
    assert!(count <= capacity, "the shim ran out of event records");
    let events: Vec<Event> = raw[..count].iter().map(Event::decode).collect();
    GpsdoRun {
        status,
        events,
        board: take_board(),
        dac_before,
    }
}

/// The shim's measurement read: loads the next scripted word into the board, or reports
/// the script spent (-1).
#[unsafe(no_mangle)]
extern "C" fn rsl_oracle_gpsdo_next_measurement() -> c_int {
    let Some(word) = SCRIPT
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .pop_front()
    else {
        return -1;
    };
    with_board(|board| {
        board.set_clock_measurement(word);
        0
    })
}

/// The board's trace length, marking where a measurement read begins.
#[unsafe(no_mangle)]
extern "C" fn rsl_oracle_gpsdo_trace_mark() -> usize {
    BOARD
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .map_or(0, |board| board.trace().len())
}
