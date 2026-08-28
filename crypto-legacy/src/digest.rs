//! Historical digest algorithms.
//!
//! [`sha1`] and [`md5`] are independently documented algorithms. Their ability to reproduce
//! historical digests does not make collision-sensitive uses safe. Both are classified
//! [`SecurityStatus::Broken`](crate::SecurityStatus::Broken).

pub mod md5;
pub mod sha1;
