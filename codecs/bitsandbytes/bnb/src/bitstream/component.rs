//! Component tests: one runtime adapter in isolation (the I/O ladder over the
//! bit cursors). `cargo test component` runs these alongside the other layers.

mod byte_runs;

#[cfg(feature = "std")]
mod stream_runs;

mod int_runs;

mod window;

mod incremental;
/// `bitstream_source.rs` — Generic recursion over `Source`: one derived
mod source;

/// `bitstream_seek.rs` — Spike (DESIGN §11): seeking is free on the in-memory cursor, and a
mod seek;

/// `bitstream_entry.rs` — Entry points: `decode`/`peek`/`decode_exact`/
mod entry;

/// `bitstream_errors.rs` — Position-aware errors: a decode/encode failure reports the
mod errors;

/// `bin_io_adapter.rs` — `Source::as_read` / `Sink::as_write`: hand a bnb cursor to `std::io`-based code
mod io_adapter;

/// `bin_buf_source.rs` — `BufSource`: a seekable `Source` over a forward `Read`. It
mod buf_source;

/// `bin_seek_reader.rs` — `SeekReader`: a `SeekSource` over a `Read + Seek` (a file-like)
mod seek_reader;

/// `BufSeekReader` — the owned, position-tracking `SeekSource`: the seek economy that
/// motivates it, error parity with `SeekReader`, and recovery after I/O errors. (Value
/// parity over random operations is `tests/seek_reader_parity.rs`.)
mod buf_seek_reader;

/// `bitbuf.rs` — `BitBuf` — a push/pull, bit-aware incremental decode buffer.
mod bitbuf;

#[cfg(feature = "bytes")]
/// `bin_bytes.rs` — `bytes` integration (the `bytes` feature): zero-copy
mod bytes_adapters;
