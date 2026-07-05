//! **demos** — runnable dual-use demonstrations for the RSL protocol stack.
//!
//! This crate has no public API; it exists as a home for `examples/` that **compose forged
//! packets** with the protocol crates and **inject** them via `rawsock`'s privileged backends:
//!
//! - [`spoof_udp`](../spoof_udp) — a UDP datagram with a **forged source IP** (impersonation),
//!   sent through a raw L3 socket.
//! - [`forge_arp`](../forge_arp) — a gratuitous **ARP reply** claiming a victim's IP is at the
//!   attacker's MAC (cache poisoning), sent through a raw L2 socket.
//!
//! Composing always works; the actual send needs `CAP_NET_RAW` (each demo falls back to just
//! printing the composed bytes when unprivileged). Run one with, e.g.:
//!
//! ```text
//! cargo run -p demos --example spoof_udp
//! sudo -E cargo run -p demos --example forge_arp -- eth0
//! ```
//!
//! These are for **authorized testing only** — spoofing and ARP poisoning on a network you do
//! not own or have permission to test is illegal.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
