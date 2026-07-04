//! `rawsock` — dual-use, layered raw-packet I/O.
//!
//! Transmit **exactly the bytes you give it** at a chosen layer — an ordinary L4
//! payload today, a hand-built IP header (L3) or a forged Ethernet frame (L2) as the
//! privileged backends land — for spoofing, fuzzing, and interop testing. The
//! socket-layer half of the [RawSocketLabs] dual-use philosophy: the protocol crates
//! *encode* (compliant by default, deliberately violatable, on the [`bnb`] codec);
//! `rawsock` *transmits* without validating.
//!
//! # The contract
//!
//! [`RawIo::send_raw`] puts the given bytes on the wire verbatim — no validation, no
//! header synthesis, no checksum or length fixing. Mechanical completion is opt-in and
//! lives in the [`Protocol`] composition model's [`encode`](ProtocolExt::encode), gated
//! by the `compute` feature.
//!
//! # Layers are opt-in
//!
//! Lower layers are Cargo features. This first cut ships the unprivileged core:
//! `transport` (L4, default) plus the [`compose`] model, the [`Loopback`] test backend,
//! and [`capabilities`] probing. The privileged `network` (L3, `IPPROTO_RAW`) and `link`
//! (L2, `AF_PACKET`) backends land with the header-forging protocols (IP/ICMP/ARP) — see
//! `ROADMAP.md`. A consumer that doesn't enable a layer cannot construct that socket:
//! misuse is a compile error, not a warning.
//!
//! # Upstream crates
//!
//! Syscalls go through [`rustix`](https://docs.rs/rustix) (safe, Linux-only, optional) —
//! **not** `libc`/`socket2`. The core here is 100% safe (`#![forbid(unsafe_code)]`). The
//! future `link` backend needs `libc` only for the `AF_PACKET` `sockaddr_ll` bind +
//! `if_nametoindex`; that is the sole planned FFI, isolated to that module, and to be
//! re-checked against rustix's `netdevice`/link support first (it may eliminate `libc`
//! entirely). See `DESIGN.md`.
//!
//! [`bnb`]: https://github.com/RawSocketLabs/bitsandbytes
//! [RawSocketLabs]: https://github.com/RawSocketLabs
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::fmt;
use std::io;

pub mod capability;
pub mod compose;
pub mod loopback;

pub use capability::{Capabilities, capabilities};
pub use compose::{Context, Protocol, ProtocolExt, Pseudo, internet_checksum};
pub use loopback::Loopback;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(all(target_os = "linux", feature = "network"))]
pub use linux::network::NetworkSocket;
#[cfg(all(target_os = "linux", feature = "transport"))]
pub use linux::transport::TransportSocket;

/// The layer at which raw bytes are transmitted — i.e. how much of the stack the
/// caller supplies versus the kernel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    /// L2: the caller supplies a full Ethernet frame; the kernel does nothing.
    Link,
    /// L3: the caller supplies a full IP header + payload; the kernel does L2.
    Network,
    /// L4: the caller supplies a transport payload; the kernel does IP + L2.
    Transport,
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Layer::Link => "link (L2)",
            Layer::Network => "network (L3)",
            Layer::Transport => "transport (L4)",
        })
    }
}

/// The dual-use sink: transmits caller-supplied bytes verbatim at one layer.
///
/// `send_raw` is the escape hatch that never validates; higher-level senders (the
/// [`Protocol`] model) compute derived fields first, then hand the bytes here.
pub trait RawIo {
    /// Transmit exactly `bytes` at this handle's layer. Returns the number of bytes
    /// written. Performs no validation, synthesis, or fix-up.
    ///
    /// # Errors
    /// Any underlying transmit failure ([`io::Error`]).
    fn send_raw(&mut self, bytes: &[u8]) -> io::Result<usize>;

    /// Receive the next frame/packet (layer-framed) into `buf`; returns its length.
    ///
    /// # Errors
    /// Any underlying receive failure ([`io::Error`]); [`io::ErrorKind::WouldBlock`]
    /// when nothing is available on a non-blocking handle.
    fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize>;

    /// The layer this handle operates at.
    fn layer(&self) -> Layer;
}

/// Why opening a layer failed — distinguishable so callers can degrade
/// (`Link` → `Network` → `Transport`) or print an actionable message.
#[derive(Debug)]
pub enum OpenError {
    /// The layer needs `CAP_NET_RAW`/root that the process doesn't have.
    PermissionDenied,
    /// This OS/build cannot offer the layer (e.g. the feature is off, or a platform
    /// without a native backend).
    LayerUnavailable,
    /// A required injection driver is absent (reserved for the Windows backends).
    DriverMissing,
    /// Any other I/O failure.
    Io(io::Error),
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpenError::PermissionDenied => {
                f.write_str("permission denied: this layer needs CAP_NET_RAW or root")
            }
            OpenError::LayerUnavailable => f.write_str("layer unavailable on this build/OS"),
            OpenError::DriverMissing => f.write_str("required injection driver is not installed"),
            OpenError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for OpenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            OpenError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for OpenError {
    fn from(e: io::Error) -> Self {
        match e.kind() {
            io::ErrorKind::PermissionDenied => OpenError::PermissionDenied,
            _ => OpenError::Io(e),
        }
    }
}
