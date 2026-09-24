//! The server's pluggable backend: how read and write requests are satisfied.
//!
//! TFTP itself has no notion of a filesystem — it just moves bytes. A
//! [`Handler`] supplies the policy: where a download's bytes come from, whether
//! an upload is allowed, and where its bytes go. The crate ships
//! [`MemoryStore`], an in-memory map suitable for tests, examples, and embedded
//! use; a real deployment implements [`Handler`] over a (carefully sandboxed)
//! filesystem.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::wire::ErrorCode;

/// A refusal returned by a [`Handler`], carrying the TFTP [`ErrorCode`] and
/// message the server will relay in an ERROR packet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reject {
    /// The wire error code to report.
    pub code: ErrorCode,
    /// The human-readable reason (sent as netascii).
    pub message: String,
}

impl Reject {
    /// Builds a rejection with an explicit code and message.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// `File not found` (error code 1).
    pub fn not_found() -> Self {
        Self::new(ErrorCode::FileNotFound, "file not found")
    }

    /// `Access violation` (error code 2).
    pub fn access_violation() -> Self {
        Self::new(ErrorCode::AccessViolation, "access violation")
    }

    /// `File already exists` (error code 6).
    pub fn already_exists() -> Self {
        Self::new(ErrorCode::FileAlreadyExists, "file already exists")
    }
}

/// The result of a handler operation: the value, or a [`Reject`] the server
/// turns into an ERROR packet.
pub type HandlerResult<T> = std::result::Result<T, Reject>;

/// Supplies the bytes for downloads and the destination for uploads.
///
/// Implementations must be `Send + Sync` because the server hands each transfer
/// to its own thread. The default [`authorize_write`](Handler::authorize_write)
/// permits all writes; override it to reject an upload *before* any data is
/// received (e.g. to enforce "file already exists" or read-only access).
//~ models rfc1350#7
//~ implements rfc1350#7/must.d23890 part="filesystem access is the handler's responsibility, not built in"
pub trait Handler: Send + Sync + 'static {
    /// Returns the bytes to send for a Read Request, or a [`Reject`].
    fn read(&self, filename: &str) -> HandlerResult<Vec<u8>>;

    /// Stores the complete contents of a finished Write Request, or a
    /// [`Reject`].
    fn write(&self, filename: &str, contents: Vec<u8>) -> HandlerResult<()>;

    /// Authorizes a Write Request before any data is transferred. Returning a
    /// [`Reject`] here makes the server refuse the upload immediately (the
    /// correct point to report "file exists" or "access violation"). Defaults to
    /// allowing the write.
    fn authorize_write(&self, _filename: &str) -> HandlerResult<()> {
        Ok(())
    }
}

/// An in-memory [`Handler`]: a name→bytes map behind a mutex.
///
/// Reads return [`Reject::not_found`] for an absent name; writes overwrite. Use
/// [`with_file`](MemoryStore::with_file) to preload content for a download test,
/// or read back an upload with [`get`](MemoryStore::get).
#[derive(Default)]
pub struct MemoryStore {
    files: Mutex<HashMap<String, Vec<u8>>>,
}

impl MemoryStore {
    /// Creates an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Preloads a file (builder-style), so a download can find it.
    pub fn with_file(self, name: impl Into<String>, contents: impl Into<Vec<u8>>) -> Self {
        self.files
            .lock()
            .unwrap()
            .insert(name.into(), contents.into());
        self
    }

    /// Inserts or replaces a file.
    pub fn insert(&self, name: impl Into<String>, contents: impl Into<Vec<u8>>) {
        self.files
            .lock()
            .unwrap()
            .insert(name.into(), contents.into());
    }

    /// Returns a copy of a stored file's bytes, if present.
    pub fn get(&self, name: &str) -> Option<Vec<u8>> {
        self.files.lock().unwrap().get(name).cloned()
    }
}

impl Handler for MemoryStore {
    fn read(&self, filename: &str) -> HandlerResult<Vec<u8>> {
        self.files
            .lock()
            .unwrap()
            .get(filename)
            .cloned()
            .ok_or_else(Reject::not_found)
    }

    fn write(&self, filename: &str, contents: Vec<u8>) -> HandlerResult<()> {
        self.files
            .lock()
            .unwrap()
            .insert(filename.to_string(), contents);
        Ok(())
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn memory_store_reads_writes_and_misses() {
        let store = MemoryStore::new().with_file("preloaded", b"hi".to_vec());
        assert_eq!(store.read("preloaded").unwrap(), b"hi");
        assert_eq!(store.read("absent"), Err(Reject::not_found()));

        store.write("uploaded", b"data".to_vec()).unwrap();
        assert_eq!(store.get("uploaded"), Some(b"data".to_vec()));
    }
}
