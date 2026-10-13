//! libusdr's `usdr_dm_create` TX sources (`tools/usdr_dm_create.c`, compiled whole by
//! `shim/dm_create.c`): its producer threads, run until the oracle has taken the packets it
//! wants from their ring, and its `do_transmit`, run on a TX stream the oracle created.

use std::ffi::{c_int, c_uint, c_void};
use std::ptr;
use std::sync::{Mutex, PoisonError};

use super::{EINVAL, Oracle, Stream, resume_callback_panic};

/// Serialises the tool's producers: they share the tool's globals (channel 0's ring, input
/// and file, the stop flag).
static TOOL: Mutex<()> = Mutex::new(());

/// `TXF_READ_FILE_EOF`: the file reader's packet ends its file.
pub const END_OF_FILE: u32 = 1;

/// Which producer thread the tool starts for a channel, and what it starts it with.
#[derive(Clone, Copy, Debug)]
pub enum Producer<'a> {
    /// `freq_gen_thread_ci16`: the `ci16` tone, or the chirp when `chirp_steps` is nonzero.
    Ci16(ThreadInput),
    /// `freq_gen_thread_ci16_lut` (`-J`): the six-entry table.
    Table(ThreadInput),
    /// `freq_gen_thread_cf32`: the `cf32` tone, whose amplitude is `gain` as it stands.
    Cf32(ThreadInput),
    /// `disk_read_thread` (`-I`, `-o` to cycle) over a file holding `bytes`, reading
    /// `block` bytes a packet.
    File {
        /// The file's contents.
        bytes: &'a [u8],
        /// Rewind at the end instead of stopping (`tx_file_cycle`).
        cycle: bool,
        /// Bytes a packet (`s_tx_blksz`, the TX stream's packet bytes).
        block: u32,
    },
}

/// The fields of the tool's `tx_thread_input_t` its generators read.
#[derive(Clone, Copy, Debug, Default)]
pub struct ThreadInput {
    /// Sample rate, for the chirp's steps (`samplerate`).
    pub samplerate: u32,
    /// Samples a packet (`samples_count`).
    pub samples: u32,
    /// dBFS for the `ci16` generators, the amplitude itself for `cf32` (`gain`).
    pub gain: f32,
    /// Start phase in turns (`start_phase`).
    pub start_phase: f64,
    /// Phase step per sample in turns (`delta_phase`).
    pub delta_phase: f64,
    /// Chirp lower frequency in Hz (`chirp_freq0`).
    pub chirp_freq0: f64,
    /// Chirp upper frequency in Hz (`chirp_freq1`).
    pub chirp_freq1: f64,
    /// Chirp steps; 0 for a tone (`chirp_steps`).
    pub chirp_steps: i32,
}

/// One packet a producer posted: its payload (the header's `len` bytes) and flags.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packet {
    /// The payload.
    pub bytes: Vec<u8>,
    /// `TXF_*` flags: [`END_OF_FILE`], or 2 for a read error.
    pub flags: u32,
}

/// `struct rsl_gen` in the shim.
#[repr(C)]
struct RslGen {
    kind: c_int,
    samplerate: c_uint,
    samples_count: c_uint,
    gain: f32,
    start_phase: f64,
    delta_phase: f64,
    chirp_freq0: f64,
    chirp_freq1: f64,
    chirp_steps: i32,
    file: *const c_void,
    file_len: usize,
    cycle: bool,
    block: c_uint,
}

unsafe extern "C" {
    fn rsl_oracle_dm_create_phase(turns: f64) -> i32;
    fn rsl_oracle_dm_create_gain(dbfs: f32) -> i16;
    fn rsl_oracle_dm_create_generate(
        g: *const RslGen,
        payload: c_uint,
        packets: c_uint,
        out: *mut u8,
        lens: *mut u32,
        flags: *mut u32,
    ) -> c_int;
    fn rsl_oracle_dm_create_transmit(
        stream: Stream,
        g: *const RslGen,
        iterations: c_uint,
        nots: bool,
        olens: *mut c_uint,
        stopped: *mut bool,
    ) -> c_int;
}

impl Producer<'_> {
    /// The shim's description; `bytes` must outlive its use.
    fn raw(&self) -> RslGen {
        let (kind, input, file) = match *self {
            Self::Ci16(input) => (0, input, None),
            Self::Table(input) => (1, input, None),
            Self::Cf32(input) => (2, input, None),
            Self::File {
                bytes,
                cycle,
                block,
            } => (3, ThreadInput::default(), Some((bytes, cycle, block))),
        };
        let (file, file_len, cycle, block) = file
            .map_or((ptr::null(), 0, false, 0), |(bytes, cycle, block)| {
                (bytes.as_ptr().cast::<c_void>(), bytes.len(), cycle, block)
            });
        RslGen {
            kind,
            samplerate: input.samplerate,
            samples_count: input.samples,
            gain: input.gain,
            start_phase: input.start_phase,
            delta_phase: input.delta_phase,
            chirp_freq0: input.chirp_freq0,
            chirp_freq1: input.chirp_freq1,
            chirp_steps: input.chirp_steps,
            file,
            file_len,
            cycle,
            block,
        }
    }
}

/// The tool's `WVLT_CONVPHASE_F32_I32(turns)`.
///
/// # Safety
///
/// `turns` must be where the C conversion is defined: above -½ and at most just past 1.
#[must_use]
pub unsafe fn phase(turns: f64) -> i32 {
    // SAFETY: a pure computation; the caller keeps it defined.
    unsafe { rsl_oracle_dm_create_phase(turns) }
}

/// The tool's `ci16` gain, `DBFS_TO_AMPLITUDE(dbfs, MAX_TXGEN_CI16_AMPL)`.
///
/// # Safety
///
/// `dbfs` must be where the C conversion to `int16_t` is defined: not NaN, and not so far
/// above 0 dBFS that the gain passes 32767.
#[must_use]
pub unsafe fn gain(dbfs: f32) -> i16 {
    // SAFETY: a pure computation; the caller keeps it defined.
    unsafe { rsl_oracle_dm_create_gain(dbfs) }
}

/// Runs `producer` on channel 0 and takes up to `packets` packets of at most `payload`
/// bytes from its ring, stopping after one that ends its file.
///
/// # Panics
///
/// If the shim fails (a producer that posts nothing for a second, a packet larger than
/// `payload`, no temporary file), or the sizes exceed `unsigned`.
#[must_use]
pub fn generate(producer: &Producer<'_>, payload: usize, packets: usize) -> Vec<Packet> {
    let raw = producer.raw();
    let mut out = vec![0; payload * packets];
    let mut lens = vec![0; packets];
    let mut flags = vec![0; packets];
    let payload_c = c_uint::try_from(payload).expect("fits unsigned");
    let packets_c = c_uint::try_from(packets).expect("fits unsigned");
    let _tool = TOOL.lock().unwrap_or_else(PoisonError::into_inner);
    // SAFETY: `raw` and the file bytes it points to outlive the call; `out` holds
    // `packets` packets of `payload` bytes, `lens` and `flags` `packets` words each, the
    // most the shim writes.
    let taken = unsafe {
        rsl_oracle_dm_create_generate(
            &raw const raw,
            payload_c,
            packets_c,
            out.as_mut_ptr(),
            lens.as_mut_ptr(),
            flags.as_mut_ptr(),
        )
    };
    let taken =
        usize::try_from(taken).unwrap_or_else(|_| panic!("the tool's producer failed: {taken}"));
    out.chunks(payload)
        .zip(lens.iter().zip(&flags))
        .take(taken)
        .map(|(packet, (&len, &flags))| Packet {
            bytes: packet[..usize::try_from(len).expect("fits")].to_vec(),
            flags,
        })
        .collect()
}

/// What the tool's TX loop did on the oracle's stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transmitted {
    /// The samples each iteration sent (`do_transmit`'s `olen`; 0 sends nothing).
    pub samples: Vec<u32>,
    /// Whether the last iteration's `do_transmit` ended the loop (a file's end, a failed
    /// send).
    pub stopped: bool,
}

impl Oracle {
    /// Runs the tool's TX-only loop for up to `iterations` iterations on the TX stream:
    /// `producer` on channel 0, then `do_transmit` each iteration, timed from one packet's
    /// samples on (`start_tx_delay`) or "now" with `nots`.
    ///
    /// # Errors
    ///
    /// The negative errno the shim returned; `-EINVAL` with no TX stream.
    ///
    /// # Panics
    ///
    /// If `iterations` exceeds `unsigned`.
    pub fn transmit_generated(
        &mut self,
        producer: &Producer<'_>,
        iterations: usize,
        nots: bool,
    ) -> Result<Transmitted, i32> {
        let Some(stream) = self.tx else {
            return Err(-EINVAL);
        };
        let raw = producer.raw();
        let mut samples = vec![0; iterations];
        let mut stopped = false;
        let count = c_uint::try_from(iterations).expect("fits unsigned");
        let _tool = TOOL.lock().unwrap_or_else(PoisonError::into_inner);
        // SAFETY: `stream` is live, `raw` and its file bytes outlive the call, `samples`
        // holds `iterations` words, the most the shim writes, and `stopped` is valid.
        let ran = unsafe {
            rsl_oracle_dm_create_transmit(
                stream.as_ptr(),
                &raw const raw,
                count,
                nots,
                samples.as_mut_ptr(),
                &raw mut stopped,
            )
        };
        resume_callback_panic();
        let ran = usize::try_from(ran).map_err(|_| ran)?;
        samples.truncate(ran);
        Ok(Transmitted { samples, stopped })
    }
}
