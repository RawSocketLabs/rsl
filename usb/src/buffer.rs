//! Transfer buffers: memory the kernel transfers into or out of, mapped from the device where
//! the kernel allows it so data moves without a copy.

use std::ops::Deref;
use std::ptr::NonNull;
use std::slice;

/// A transfer's memory and how much of it holds data.
///
/// For an IN transfer the data is what the device sent; for an OUT transfer, what to send,
/// set with [`Buffer::as_mut_slice`] and [`Buffer::set_len`]. Reused by submitting it again.
#[derive(Debug)]
pub struct Buffer {
    /// Where the bytes live.
    storage: Storage,

    /// How many bytes, from the start, hold data.
    len: usize,
}

/// Where a buffer's bytes live.
#[derive(Debug)]
enum Storage {
    /// Ordinary memory; the kernel copies to and from it.
    Heap(Box<[u8]>),

    /// Memory mapped from the device, which the controller reads and writes directly.
    Mapped(Mapping),
}

impl Buffer {
    /// A zeroed buffer of `capacity` bytes in ordinary memory, holding no data.
    pub(crate) fn heap(capacity: usize) -> Self {
        Self {
            storage: Storage::Heap(vec![0; capacity].into_boxed_slice()),
            len: 0,
        }
    }

    /// A buffer over device-mapped memory, holding no data.
    pub(crate) fn mapped(mapping: Mapping) -> Self {
        Self {
            storage: Storage::Mapped(mapping),
            len: 0,
        }
    }

    /// The most bytes the buffer holds.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.all().len()
    }

    /// Whether the controller moves data straight into and out of this buffer. When the
    /// kernel cannot map more device memory, buffers fall back to ordinary memory, which
    /// costs a copy per transfer.
    #[must_use]
    pub fn is_zero_copy(&self) -> bool {
        matches!(self.storage, Storage::Mapped(_))
    }

    /// The whole buffer, to fill before an OUT transfer.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        match &mut self.storage {
            Storage::Heap(bytes) => bytes,
            Storage::Mapped(mapping) => mapping.bytes_mut(),
        }
    }

    /// Marks the first `len` bytes as data.
    ///
    /// # Panics
    ///
    /// If `len` exceeds the capacity.
    pub fn set_len(&mut self, len: usize) {
        assert!(
            len <= self.capacity(),
            "a buffer cannot hold more than its capacity"
        );
        self.len = len;
    }

    /// The whole buffer.
    fn all(&self) -> &[u8] {
        match &self.storage {
            Storage::Heap(bytes) => bytes,
            Storage::Mapped(mapping) => mapping.bytes(),
        }
    }

    /// The start of the buffer, for the kernel.
    pub(crate) fn as_mut_ptr(&mut self) -> *mut u8 {
        self.as_mut_slice().as_mut_ptr()
    }
}

impl Deref for Buffer {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.all()[..self.len]
    }
}

/// A region of device memory mapped into this process; unmapped on drop.
#[derive(Debug)]
pub(crate) struct Mapping {
    /// The region's start.
    base: NonNull<u8>,

    /// Its length in bytes.
    len: usize,
}

// SAFETY: a `Mapping` is the only handle to its region, so moving it to another thread moves
// all access with it.
unsafe impl Send for Mapping {}
// SAFETY: shared references only read the region (`bytes`); writes need `&mut`.
unsafe impl Sync for Mapping {}

impl Mapping {
    /// Takes ownership of a mapped region.
    ///
    /// # Safety
    ///
    /// `base` must start a readable, writable mapping of `len` bytes that nothing else
    /// references and that stays mapped until this value unmaps it.
    pub(crate) unsafe fn new(base: NonNull<u8>, len: usize) -> Self {
        Self { base, len }
    }

    /// The region.
    fn bytes(&self) -> &[u8] {
        // SAFETY: `len` mapped bytes owned by `self` (see `new`). The controller writes them
        // only while a transfer is in flight, and then the queue owns the buffer, so no
        // reference to it exists.
        unsafe { slice::from_raw_parts(self.base.as_ptr(), self.len) }
    }

    /// The region, writable.
    fn bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: as `bytes`, and `&mut self` makes this the only reference.
        unsafe { slice::from_raw_parts_mut(self.base.as_ptr(), self.len) }
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        // SAFETY: the region was mapped with this base and length (see `new`) and no
        // reference into it outlives `self`. The kernel keeps the memory itself alive while a
        // transfer uses it, so unmapping never frees it under the controller.
        let _ = unsafe { rustix::mm::munmap(self.base.as_ptr().cast(), self.len) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_buffer_holds_no_data_until_its_length_is_set() {
        let mut buffer = Buffer::heap(8);
        assert!(buffer.is_empty(), "a fresh buffer exposes no bytes");
        buffer.as_mut_slice()[..3].copy_from_slice(&[1, 2, 3]);
        buffer.set_len(3);
        assert_eq!(&*buffer, &[1, 2, 3]);
        assert_eq!(buffer.capacity(), 8);
    }

    #[test]
    #[should_panic(expected = "capacity")]
    fn a_length_past_the_capacity_is_refused() {
        Buffer::heap(4).set_len(5);
    }
}
