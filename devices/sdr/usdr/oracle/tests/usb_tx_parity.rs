//! Pinned C USB TX framing, status throttling and lifecycle, over the same hardware-free
//! buffer boundary: `shim/usb_tx.c` runs libusdr's own `_usb_uram_init_txstream`,
//! `usb_uram_send_dma_get`, `usb_uram_send_dma_commit` and `usb_uram_stream_deinitialize`,
//! exported verbatim by `vendor.sh`; usdr's `transport/usb/tx.rs` runs unchanged.
#![cfg(target_os = "linux")]

use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use usdr::dsp::{Converter, SampleFormat};
use usdr::lowlevel::BusError;
use usdr_oracle::conv;

/// Satisfies the production TX module's normal imports without copying that module.
mod lowlevel {
    pub(crate) use usdr::lowlevel::{BusError, TxFill, usb_error};
}

#[path = "../../src/transport/usb/tx.rs"]
mod tx;

unsafe extern "C" {
    /// A zeroed device with libusdr's TX statistics started, or null.
    fn rsl_oracle_usb_tx_new() -> *mut c_void;
    /// Frees a device and any buffers it holds.
    fn rsl_oracle_usb_tx_free(handle: *mut c_void);
    /// Opens TX; fills six events: transfers, buffers, buffer bytes, endpoint, MTU, channel.
    fn rsl_oracle_usb_tx_open(
        handle: *mut c_void,
        block: u32,
        bits_per_sym: u32,
        events: *mut u32,
    ) -> i32;
    /// Sets what the TX core's four status registers read next.
    fn rsl_oracle_usb_tx_set_status(handle: *mut c_void, words: *const u32);
    /// One get, waiting up to `timeout_ms`, payload write and commit; copies the posted
    /// frame out.
    fn rsl_oracle_usb_tx_send(
        handle: *mut c_void,
        payload: *const u8,
        size: u32,
        timestamp: u64,
        timeout_ms: u32,
        status: *mut u32,
        status_size: *mut u32,
        frame: *mut u8,
        frame_size: *mut u32,
    ) -> i32;
    /// The device finishes the oldest transfer.
    fn rsl_oracle_usb_tx_finish(handle: *mut c_void);
    /// Status reads so far, with the last one's register and word count.
    fn rsl_oracle_usb_tx_reads(handle: *mut c_void, addr: *mut u32, words: *mut u32) -> u32;
    /// Closes TX; the close count, or the error.
    fn rsl_oracle_usb_tx_close(handle: *mut c_void) -> i32;
}

/// Linux `ETIMEDOUT`, negated as libusdr returns it.
const TIMED_OUT: i32 = -110;
/// Linux `EAGAIN`, likewise: `sem_trywait` finding no free buffer.
const AGAIN: i32 = -11;
/// The wait both sides pass, but for the zero-timeout case.
const WAIT: Duration = Duration::from_millis(1);
/// The largest frame: `MAX_TX_BUFFER_SZ` and the header.
const MAX_FRAME: usize = 126_976 + 16;

/// One libusdr USB device, used by one test at a time: `usb_uram_send_dma_get` keeps a
/// process-wide, non-atomic counter.
struct Reference {
    /// The shim's device.
    handle: *mut c_void,
    /// Serialises the C calls across tests.
    _guard: MutexGuard<'static, ()>,
}

/// What libusdr sent for one block.
#[derive(Debug, PartialEq, Eq)]
struct Sent {
    /// The whole transfer, header included.
    frame: Vec<u8>,
    /// The status words reported with the buffer.
    status: [u32; 4],
}

impl Reference {
    /// A fresh device.
    fn new() -> Self {
        static REFERENCE: Mutex<()> = Mutex::new(());
        let guard = REFERENCE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // SAFETY: allocates and initialises a device the shim owns until `free`.
        let handle = unsafe { rsl_oracle_usb_tx_new() };
        assert!(!handle.is_null(), "the shim allocates its device");
        Self {
            handle,
            _guard: guard,
        }
    }

    /// Opens TX for `block` bytes of `bits_per_sym`-bit samples; the six events or errno.
    fn open(&mut self, block: u32, bits_per_sym: u32) -> Result<[u32; 6], i32> {
        let mut events = [0; 6];
        // SAFETY: the handle is live and `events` holds the six words the shim writes.
        let status = unsafe {
            rsl_oracle_usb_tx_open(self.handle, block, bits_per_sym, events.as_mut_ptr())
        };
        if status == 0 { Ok(events) } else { Err(status) }
    }

    /// What the status registers read next.
    fn set_status(&mut self, words: [u32; 4]) {
        // SAFETY: the handle is live and `words` holds the four words the shim copies.
        unsafe { rsl_oracle_usb_tx_set_status(self.handle, words.as_ptr()) };
    }

    /// Sends `payload` to play at `time` (`None`: libusdr's -1), waiting up to [`WAIT`].
    fn send(&mut self, payload: &[u8], time: Option<u64>) -> Result<Sent, i32> {
        self.send_waiting(payload, time, WAIT)
    }

    /// Sends `payload` to play at `time`, waiting up to `timeout` for a buffer.
    fn send_waiting(
        &mut self,
        payload: &[u8],
        time: Option<u64>,
        timeout: Duration,
    ) -> Result<Sent, i32> {
        let mut frame = vec![0; MAX_FRAME];
        let (mut status, mut status_size, mut frame_size) = ([0; 4], 0, 0);
        // SAFETY: the handle is live; the payload is read for its length, at most one
        // block (the callers' blocks are at most `MAX_TX_BUFFER_SZ`); `status` takes the
        // 16 bytes requested and `frame` the largest frame the stream can post.
        let result = unsafe {
            rsl_oracle_usb_tx_send(
                self.handle,
                payload.as_ptr(),
                u32::try_from(payload.len()).expect("a block fits u32"),
                time.unwrap_or(u64::MAX),
                u32::try_from(timeout.as_millis()).expect("a short wait"),
                status.as_mut_ptr(),
                &raw mut status_size,
                frame.as_mut_ptr(),
                &raw mut frame_size,
            )
        };
        if result != 0 {
            return Err(result);
        }
        assert_eq!(status_size, 16, "libusdr reports all four status words");
        frame.truncate(usize::try_from(frame_size).expect("fits usize"));
        Ok(Sent { frame, status })
    }

    /// The device finishes the oldest transfer.
    fn finish(&mut self) {
        // SAFETY: the handle is live.
        unsafe { rsl_oracle_usb_tx_finish(self.handle) };
    }

    /// Status reads so far, the last one's register and its word count.
    fn reads(&mut self) -> (u32, u32, u32) {
        let (mut addr, mut words) = (0, 0);
        // SAFETY: the handle is live and both outputs are valid words.
        let reads = unsafe { rsl_oracle_usb_tx_reads(self.handle, &raw mut addr, &raw mut words) };
        (reads, addr, words)
    }

    /// Closes TX; the close count.
    fn close(&mut self) -> i32 {
        // SAFETY: the handle is live.
        unsafe { rsl_oracle_usb_tx_close(self.handle) }
    }
}

impl Drop for Reference {
    fn drop(&mut self) {
        // SAFETY: the handle came from `rsl_oracle_usb_tx_new` and is freed once.
        unsafe { rsl_oracle_usb_tx_free(self.handle) };
    }
}

/// What usdr's endpoint saw, shared with the test.
#[derive(Default)]
struct State {
    /// Sent transfers the device has not finished, oldest first.
    in_flight: VecDeque<Vec<u8>>,
    /// Transfers the device may finish before the next wait.
    finished: usize,
    /// Every frame sent.
    frames: Vec<Vec<u8>>,
    /// Cancellations.
    cancellations: usize,
}

/// Hardware-free endpoint with the same transfer ownership as `OutQueue`.
struct Endpoint(Arc<Mutex<State>>);

impl tx::TxQueue for Endpoint {
    type Buffer = Vec<u8>;

    fn bytes(buffer: &mut Vec<u8>) -> &mut [u8] {
        buffer
    }

    fn alloc(&mut self, capacity: usize) -> Result<Vec<u8>, BusError> {
        Ok(vec![0; capacity])
    }

    fn submit(&mut self, buffer: Vec<u8>, len: usize) -> Result<(), BusError> {
        let mut state = self.0.lock().unwrap();
        state.frames.push(buffer[..len].to_vec());
        state.in_flight.push_back(buffer);
        Ok(())
    }

    fn wait(&mut self, _: Duration) -> Result<Option<Vec<u8>>, BusError> {
        let mut state = self.0.lock().unwrap();
        if state.finished == 0 {
            return Ok(None);
        }
        state.finished -= 1;
        Ok(state.in_flight.pop_front())
    }

    fn cancel(&mut self) -> Result<(), BusError> {
        let mut state = self.0.lock().unwrap();
        state.in_flight.clear();
        state.finished = 0;
        state.cancellations += 1;
        Ok(())
    }
}

/// usdr's side: the endpoint, its device's status throttle and register file.
struct Rsl {
    /// The endpoint slot, as `UsbBus` keeps it.
    slot: Option<tx::Tx<Endpoint>>,
    /// Per device, as in libusdr.
    status: tx::TxStatus,
    /// What the status registers read.
    register: [u32; 4],
    /// Status reads so far.
    reads: u32,
    /// The endpoint's record.
    state: Arc<Mutex<State>>,
}

impl Rsl {
    /// A device with no stream.
    fn new() -> Self {
        Self {
            slot: None,
            status: tx::TxStatus::default(),
            register: [0; 4],
            reads: 0,
            state: Arc::default(),
        }
    }

    /// Opens TX as `UsbBus::tx_stream_open` does.
    fn open(&mut self, block: u32, sample_bytes: u32) -> Result<(), BusError> {
        let state = Arc::clone(&self.state);
        tx::Tx::open_endpoint(&mut self.slot, block, sample_bytes, || Ok(Endpoint(state)))
    }

    /// Sends through `UsbBus::tx_stream_send`'s own sequence, [`tx::TxDevice::send`]: the
    /// block is what `write` puts in the lent buffer.
    fn send(
        &mut self,
        time: Option<u64>,
        timeout: Duration,
        write: &mut dyn FnMut(&mut [u8]) -> usize,
    ) -> Result<Sent, BusError> {
        let mut reported = None;
        tx::TxDevice::send(self, timeout, time, &mut |buffer, status| {
            reported = status;
            write(buffer)
        })?;
        let frame = self
            .state
            .lock()
            .unwrap()
            .frames
            .last()
            .cloned()
            .expect("sent");
        Ok(Sent {
            frame,
            status: reported.expect("USB reports status words"),
        })
    }

    /// Sends `payload` unchanged, waiting up to `timeout` for a buffer.
    fn send_bytes(
        &mut self,
        payload: &[u8],
        time: Option<u64>,
        timeout: Duration,
    ) -> Result<Sent, BusError> {
        self.send(time, timeout, &mut |buffer| {
            buffer[..payload.len()].copy_from_slice(payload);
            payload.len()
        })
    }

    /// The device finishes the oldest transfer.
    fn finish(&mut self) {
        self.state.lock().unwrap().finished += 1;
    }

    /// The open endpoint.
    fn tx(&mut self) -> &mut tx::Tx<Endpoint> {
        self.slot.as_mut().expect("TX is open")
    }
}

impl tx::TxDevice for Rsl {
    fn acquire(&mut self, timeout: Duration) -> Result<(), BusError> {
        self.tx().acquire(timeout)
    }

    fn read_status(&mut self) -> Result<[u32; 4], BusError> {
        self.reads += 1;
        Ok(self.register)
    }

    fn status(&mut self) -> &mut tx::TxStatus {
        &mut self.status
    }

    fn commit(
        &mut self,
        time: Option<u64>,
        status: Option<[u32; 4]>,
        fill: &mut lowlevel::TxFill<'_>,
    ) -> Result<(), BusError> {
        self.tx().commit(time, status, fill)
    }
}

/// `len` bytes counting up from `seed`.
fn pattern(len: usize, seed: usize) -> Vec<u8> {
    (0..len).map(|n| (n * 7 + seed).to_le_bytes()[0]).collect()
}

#[test]
fn block_limits_and_stream_setup_match_the_pinned_c_transmitter() {
    let mut reference = Reference::new();
    for block in [63, 64, 4096, 126_976, 126_977] {
        let expected = reference.open(block, 32);
        let mut rsl = Rsl::new();
        let opened = rsl.open(block, 4);
        assert_eq!(opened.is_ok(), expected.is_ok(), "block of {block} bytes");
        let Ok(events) = expected else {
            assert_eq!(expected, Err(-22), "libusdr refuses with EINVAL");
            continue;
        };
        assert_eq!(
            events,
            [32, 32, block + 16, 0x03, block, 1],
            "32 transfers of block and header on endpoint 0x03, MTU the block, TX channel"
        );
        assert_eq!(reference.close(), 1, "libusdr releases its buffers");
        tx::Tx::close_endpoint(&mut rsl.slot).expect("drains");
        assert_eq!(rsl.state.lock().unwrap().cancellations, 1);
    }
    // Intentional divergence: libusdr takes 0 for its largest block; the driver never asks.
    let events = reference.open(0, 32).expect("libusdr opens its largest");
    assert_eq!(events[4], 126_976);
    assert!(Rsl::new().open(0, 4).is_err());
}

#[test]
fn frames_match_the_pinned_c_for_full_and_partial_blocks_at_every_time() {
    let times = [
        None,
        Some(0),
        Some(1000),
        Some(1 << 32 | 5),
        Some((1 << 47) - 1),
        Some(0x7fff_1234_5678_9abc),
    ];
    // `ci16` on one and two channels, then the extended core's `ci12` on one and two.
    let shapes: [(u32, u32, &[u32]); 4] = [
        (32, 4, &[4, 8, 400, 2048, 4088, 4096]),
        (64, 8, &[8, 400, 2048, 4088, 4096]),
        (24, 3, &[3, 402, 2046, 4092]),
        (48, 6, &[6, 402, 2046, 4092]),
    ];
    for (bits, sample_bytes, lens) in shapes {
        let mut reference = Reference::new();
        reference.open(4096, bits).expect("opens");
        let mut rsl = Rsl::new();
        rsl.open(4096, sample_bytes).expect("opens");
        for &len in lens {
            for (index, time) in times.into_iter().enumerate() {
                let len = usize::try_from(len).unwrap();
                let payload = pattern(len, index);
                let expected = reference.send(&payload, time).expect("C sends");
                let sent = rsl.send_bytes(&payload, time, WAIT).expect("usdr sends");
                assert_eq!(
                    sent.frame, expected.frame,
                    "{len} bytes of {sample_bytes}-byte samples at {time:?}"
                );
                reference.finish();
                rsl.finish();
            }
        }
    }
}

#[test]
fn every_host_format_reaches_the_wire_framed_as_libusdr_frames_it() {
    for (format, name, bytes) in [
        (SampleFormat::Ci16, "ci16", 4),
        (SampleFormat::Cf32, "cf32", 8),
        (SampleFormat::Ci12, "ci12", 3),
    ] {
        let mut reference = Reference::new();
        reference.open(4096, 32).expect("opens");
        let mut rsl = Rsl::new();
        rsl.open(4096, 4).expect("opens");
        let c_converter = conv::transform(name, "ci16", 1, 1).expect("libusdr converts it");
        let converter = Converter::new(format, SampleFormat::Ci16, 1, 1).expect("ported");
        // One sample, across the 512-sample conversion chunk, and a full 1024-sample packet.
        for samples in [1, 513, 1024] {
            let host: Vec<u8> = if format == SampleFormat::Cf32 {
                (0..samples * 2)
                    .flat_map(|n| {
                        let n = f32::from(u16::try_from(n % 2000).unwrap());
                        ((n - 1000.0) / 1000.0).to_le_bytes()
                    })
                    .collect()
            } else {
                pattern(samples * bytes, samples)
            };
            let wire = c_converter.run(&[&host], &[vec![0; samples * 4]]);
            assert!(!wire.overran);
            let time = Some(u64::try_from(samples).unwrap() * 1000);
            let expected = reference.send(&wire.outputs[0], time).expect("C sends");
            let sent = rsl
                .send(time, WAIT, &mut |buffer| {
                    let out = &mut buffer[..samples * 4];
                    converter.convert(&[&host], &mut [out]).expect("converts");
                    samples * 4
                })
                .expect("usdr sends");
            assert_eq!(sent.frame, expected.frame, "{name}, {samples} samples");
            reference.finish();
            rsl.finish();
        }
    }
}

#[test]
fn status_is_read_and_repeated_as_the_pinned_c_throttles_it_across_streams() {
    let mut reference = Reference::new();
    let mut rsl = Rsl::new();
    let payload = pattern(64, 0);
    let mut sends = 0_u32;
    let mut send_both = |reference: &mut Reference, rsl: &mut Rsl, finish: bool, wait| {
        let words = [8 | sends << 8, sends, !sends, sends << 16];
        sends += 1;
        reference.set_status(words);
        rsl.register = words;
        let expected = reference.send_waiting(&payload, None, wait);
        let sent = rsl.send_bytes(&payload, None, wait);
        if finish {
            reference.finish();
            rsl.finish();
        }
        (expected, sent)
    };
    for stream in 0..2 {
        reference.open(4096, 32).expect("opens");
        rsl.open(4096, 4).expect("opens");
        // A full ring: 32 unfinished sends, then one that times out on both, uncounted.
        for _ in 0..32 {
            let (expected, sent) = send_both(&mut reference, &mut rsl, false, WAIT);
            assert_eq!(sent.expect("sends").status, expected.expect("sends").status);
        }
        let (expected, sent) = send_both(&mut reference, &mut rsl, false, WAIT);
        assert_eq!(expected, Err(TIMED_OUT));
        assert!(matches!(sent, Err(BusError::Timeout)), "{sent:?}");
        // Without a wait, libusdr's `sem_trywait` gives EAGAIN, which its stream engine
        // returns without the status read it makes for ETIMEDOUT. The transport here
        // reports a timeout, so `Board::send` reads the status for its warning: a
        // divergence `Device::send` shows (`UT:device_tests`). Neither counts the send.
        let (expected, sent) = send_both(&mut reference, &mut rsl, false, Duration::ZERO);
        assert_eq!(expected, Err(AGAIN));
        assert!(matches!(sent, Err(BusError::Timeout)), "{sent:?}");
        for _ in 0..32 {
            reference.finish();
            rsl.finish();
        }
        for _ in 0..100 {
            let (expected, sent) = send_both(&mut reference, &mut rsl, true, WAIT);
            assert_eq!(
                sent.expect("sends").status,
                expected.expect("sends").status,
                "stream {stream}"
            );
        }
        let (reads, addr, words) = reference.reads();
        assert_eq!(reads, rsl.reads, "stream {stream}");
        assert_eq!((addr, words), (tx::STATUS_REGISTER, 4));
        assert_eq!(reference.close(), 1);
        tx::Tx::close_endpoint(&mut rsl.slot).expect("drains");
    }
    assert_eq!(
        rsl.reads, 5,
        "sends 0, 64, 128, 192 and 256 of the device read"
    );
}
