//! Rust stand-ins for libairspy's session calls, used only under Miri, which
//! cannot call C. They model the thread behaviour `Receiver` relies on: start
//! runs the callback from another thread, and stop joins it, except that a
//! release before 1.0.12 skips the join after a USB failure.

use super::{
    AIRSPY_ERROR_LIBUSB, AIRSPY_SUCCESS, AIRSPY_TRUE, AirspyDevice, RX_CONTINUE, SampleType,
    airspy_lib_version_t, airspy_transfer_t,
};
use std::cell::{Cell, RefCell};
use std::ffi::{c_int, c_void};
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

thread_local! {
    /// The libairspy revision (1.0.x) the fake reports to this test thread.
    pub(crate) static REVISION: Cell<u32> = const { Cell::new(12) };
    /// Whether started callback threads deliver samples or stay silent.
    pub(crate) static DELIVER: Cell<bool> = const { Cell::new(true) };
    /// Whether start fails before creating threads, as a failed control
    /// transfer does, leaving `streaming` clear.
    pub(crate) static FAIL_START: Cell<bool> = const { Cell::new(false) };
    /// This test thread's open fake devices, keyed by handle address. The
    /// handle is a leaked byte, so no Rust reference to it ever covers the
    /// state here; per thread, so parallel tests never see each other's.
    static DEVICES: RefCell<Vec<(usize, Device)>> = const { RefCell::new(Vec::new()) };
}

/// A callback thread and the flag that ends it.
struct Session {
    running: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

impl Session {
    /// End the callback thread and wait for it.
    fn join(self) {
        self.running.store(false, Ordering::Release);
        self.thread.join().expect("fake callback thread panicked");
    }
}

/// One open fake device.
#[derive(Default)]
struct Device {
    session: Option<Session>,
    /// Cleared by start, set by a simulated USB failure.
    usb_failed: bool,
    /// The handle's allocation, with the provenance that frees it.
    alloc: Option<SendPtr>,
}

/// Run `f` on the fake device behind `device`.
fn with<R>(device: *const AirspyDevice, f: impl FnOnce(&mut Device) -> R) -> R {
    DEVICES.with_borrow_mut(|devices| {
        let (_, entry) = devices
            .iter_mut()
            .find(|(addr, _)| *addr == device.addr())
            .expect("fake handle is open");
        f(entry)
    })
}

/// Join a device's callback thread, if any, and free its handle.
fn close_entry(entry: Device) {
    if let Some(session) = entry.session {
        session.join();
    }
    let alloc = entry.alloc.expect("opened handles record their allocation");
    // SAFETY: `alloc` is airspy_open's box, and its entry is gone.
    drop(unsafe { Box::from_raw(alloc.get().cast::<u8>()) });
}

/// A pointer kept in the device table or moved to the callback thread.
struct SendPtr(*mut c_void);

// SAFETY: the pointer is only handed back to the callback, whose contract
// makes the pointee shareable across threads.
unsafe impl Send for SendPtr {}

impl SendPtr {
    /// The pointer; a method, so the closure captures the whole `Send` value.
    fn get(&self) -> *mut c_void {
        self.0
    }
}

pub(crate) fn airspy_lib_version(version: &mut airspy_lib_version_t) {
    version.major_version = 1;
    version.minor_version = 0;
    version.revision = REVISION.get();
}

pub(crate) fn airspy_open(device: &mut *mut AirspyDevice) -> c_int {
    let alloc = Box::into_raw(Box::new(0_u8)).cast::<c_void>();
    *device = alloc.cast();
    let entry = Device {
        alloc: Some(SendPtr(alloc)),
        ..Device::default()
    };
    DEVICES.with_borrow_mut(|devices| devices.push((alloc.addr(), entry)));
    AIRSPY_SUCCESS
}

/// # Safety
///
/// `device` came from `airspy_open` and is not used again.
pub(crate) unsafe fn airspy_close(device: *mut AirspyDevice) -> c_int {
    let entry = DEVICES.with_borrow_mut(|devices| {
        let index = devices
            .iter()
            .position(|(addr, _)| *addr == device.addr())
            .expect("fake handle is open");
        devices.swap_remove(index).1
    });
    // Real libairspy would free the device and condvar under that thread.
    assert!(
        entry.session.is_none(),
        "airspy_close with an unjoined callback thread"
    );
    close_entry(entry);
    AIRSPY_SUCCESS
}

/// Close every device this test thread leaked instead of closing, joining
/// their callback threads, so Miri sees no leak.
pub(crate) fn close_leaked() {
    for (_, entry) in DEVICES.take() {
        close_entry(entry);
    }
}

pub(crate) fn airspy_set_sample_type(_device: &mut AirspyDevice, _sample: SampleType) -> c_int {
    AIRSPY_SUCCESS
}

/// # Safety
///
/// As `ffi::airspy_start_rx`: `rx_ctx` stays valid for `callback` until a
/// stop that joins.
pub(crate) unsafe fn airspy_start_rx(
    device: &mut AirspyDevice,
    callback: unsafe extern "C" fn(*mut airspy_transfer_t) -> c_int,
    rx_ctx: *mut c_void,
) -> c_int {
    // Real libairspy would race a second consumer against the orphaned one.
    assert!(
        with(device, |device| device.session.is_none()),
        "airspy_start_rx with an unjoined callback thread"
    );
    if FAIL_START.get() {
        return AIRSPY_ERROR_LIBUSB;
    }
    let running = Arc::new(AtomicBool::new(true));
    let deliver = DELIVER.get();
    let ctx = SendPtr(rx_ctx);
    let thread = thread::spawn({
        let running = Arc::clone(&running);
        move || {
            let samples = [0.5_f32; 8];
            while running.load(Ordering::Acquire) {
                if !deliver {
                    thread::yield_now();
                    continue;
                }
                let mut transfer = airspy_transfer_t {
                    device: ptr::null_mut(),
                    ctx: ctx.get(),
                    samples: samples.as_ptr().cast(),
                    sample_count: 4,
                };
                // SAFETY: `transfer` and `samples` outlive the call, and the
                // caller keeps `ctx` valid until a joining stop.
                if unsafe { callback(&raw mut transfer) } != RX_CONTINUE {
                    break;
                }
                thread::yield_now();
            }
        }
    });
    with(device, |device| {
        device.usb_failed = false;
        device.session = Some(Session { running, thread });
    });
    AIRSPY_SUCCESS
}

pub(crate) fn airspy_stop_rx(device: &mut AirspyDevice) -> c_int {
    let skip_join = REVISION.get() < 12;
    let session = with(device, |device| {
        if device.usb_failed && skip_join {
            None
        } else {
            device.session.take()
        }
    });
    if let Some(session) = session {
        session.join();
    }
    AIRSPY_SUCCESS
}

pub(crate) fn airspy_is_streaming(device: &mut AirspyDevice) -> c_int {
    with(device, |device| {
        c_int::from(device.session.is_some() && !device.usb_failed) * AIRSPY_TRUE
    })
}

/// Simulate a USB failure: libairspy clears `streaming`, but its callback
/// thread may still be inside the callback.
pub(crate) fn fail_usb(device: &mut AirspyDevice) {
    with(device, |device| device.usb_failed = true);
}

/// End and join the device's callback thread, as it would end on its own
/// after a USB failure.
pub(crate) fn end_callbacks(device: &mut AirspyDevice) {
    if let Some(session) = with(device, |device| device.session.take()) {
        session.join();
    }
}
