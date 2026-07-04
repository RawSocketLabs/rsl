//! **ethertype** — the 16-bit link-layer protocol identifier (IEEE 802.3), as a `bnb`
//! [`BitEnum`](bnb::BitEnum).
//!
//! A tiny leaf crate: one enum, network byte order, dual-use (unknown values are
//! preserved as [`EtherType::Custom`], never rejected). Other link-layer crates
//! (`ethernet`, `arp`) depend on it to name their encapsulated protocol.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod ethertype;

pub use ethertype::EtherType;
