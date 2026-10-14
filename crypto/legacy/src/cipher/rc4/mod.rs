//! RC4's key scheduling and byte-at-a-time pseudorandom generation algorithms.
//!
//! > **RC4 is cryptographically broken. Never use it for new protection.**
//!
//! RC4 maintains a permutation `S` of every byte value plus two one-byte indices. Construction
//! first runs the key-scheduling algorithm (KSA): it walks `S` once, updates `j` with a repeated
//! key byte, and swaps `S[i]` with `S[j]`. Each subsequent pseudorandom-generation algorithm
//! (PRGA) step advances both indices, performs another swap, and selects one keystream byte. That
//! byte is combined with input using XOR, so encryption and decryption are the same operation.
//!
//! # A complete historical round trip
//!
//! ```
//! use rsl_crypto_legacy::cipher::rc4::{Rc4, Rc4Key};
//!
//! let key_bytes = b"historical key";
//! let mut sender = Rc4::new(Rc4Key::try_from_slice(key_bytes)?);
//! let mut receiver = Rc4::new(Rc4Key::try_from_slice(key_bytes)?);
//! let mut bytes = *b"wire bytes";
//!
//! sender.apply_keystream(&mut bytes)?;
//! assert_ne!(&bytes, b"wire bytes");
//! receiver.apply_keystream(&mut bytes)?;
//! assert_eq!(&bytes, b"wire bytes");
//! # Ok::<(), rsl_crypto_legacy::CryptoError>(())
//! ```
//!
//! This works only because both directions begin with identical key and position and consume the
//! same number of bytes. It provides no nonce, authentication, record boundaries, resynchronizing,
//! or safe key-reuse rule.
//!
//! # Discarding early output
//!
//! RFC 4345's historical SSH `arcfour128` and `arcfour256` profiles discarded the first 1,536
//! keystream bytes. [`Rc4::discard`] exposes the primitive needed to reproduce that behavior, but
//! deliberately does not apply it automatically: raw RC4, TLS RC4, and SSH Arcfour are distinct
//! protocol profiles. RFC 8758 later deprecated RC4 in SSH, and RFC 7465 prohibits negotiating it
//! in every TLS version.
//!
//! # Review and timing limits
//!
//! The implementation follows the conventional KSA/PRGA notation directly and uses no `unsafe`
//! code. RC4 inherently indexes memory with secret-dependent values; this readable implementation
//! is not constant-time. The permutation is zeroized on drop, and key ownership is non-`Clone`,
//! redacted, and zeroizing. Those lifetime measures do not repair RC4's statistical biases.
//!
//! RFC 6229 supplies the controlling interoperability vectors and offset convention. Exact
//! source links, algorithm-to-code mapping, security sources, test coverage, and exclusions are
//! recorded in the package `STANDARDS.md`.

mod key_schedule;
mod state;

pub use state::{MAX_KEY_LEN, MIN_KEY_LEN, Rc4, Rc4Key};

/// RC4's lifecycle status: keystream biases enable practical attacks in deployed protocols.
pub const SECURITY_STATUS: crate::SecurityStatus = crate::SecurityStatus::Broken;
