//! **uri** — URI value types and encodings.
//!
//! So far this is [`form`]: percent-encoding of names and values into
//! `application/x-www-form-urlencoded` and query-string form, with the set of bytes left
//! unencoded chosen explicitly by the caller.
//!
//! ```
//! use uri::form::Encoding;
//!
//! let query = Encoding::QUERY.pairs([(&b"q"[..], &b"a b&c"[..]), (b"lang", b"en-US")]);
//! assert_eq!(query, b"q=a+b%26c&lang=en-US");
//! ```

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

extern crate alloc;

pub mod form;
