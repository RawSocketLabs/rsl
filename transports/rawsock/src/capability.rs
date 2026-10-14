//! Probe what the current host + process can actually do — never assumed.

use crate::Layer;

/// Which layers this host+process can open *right now* (e.g. `link`/`network` require
/// `CAP_NET_RAW`). Determined by attempting a cheap socket open per layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    /// L2 (`AF_PACKET`) is openable.
    pub link: bool,
    /// L3 (`SOCK_RAW`) is openable.
    pub network: bool,
    /// L4 (ordinary sockets) is openable.
    pub transport: bool,
}

impl Capabilities {
    /// Whether the given `layer` is openable.
    #[must_use]
    pub fn allows(&self, layer: Layer) -> bool {
        match layer {
            Layer::Link => self.link,
            Layer::Network => self.network,
            Layer::Transport => self.transport,
        }
    }
}

/// Probe the openable layers by attempting (and immediately dropping) a socket of each
/// kind, mapping `EPERM`/`EACCES` to `false`. Probes all three layers regardless of which
/// backends are compiled — it reports what the *host* allows, not what this build ships.
/// Available when the socket backend (`transport`) is enabled; otherwise nothing is
/// openable and a stub returns all-`false`.
#[cfg(all(target_os = "linux", feature = "transport"))]
#[must_use]
pub fn capabilities() -> Capabilities {
    use rustix::net::{AddressFamily, RawProtocol, SocketFlags, SocketType, ipproto, socket_with};

    fn can(family: AddressFamily, ty: SocketType, proto: Option<rustix::net::Protocol>) -> bool {
        socket_with(family, ty, SocketFlags::empty(), proto).is_ok()
    }

    // IPPROTO_RAW (255) ⇒ IP_HDRINCL; htons(ETH_P_ALL) for the packet socket.
    let ip_raw = Some(ipproto::RAW);
    let eth_all =
        RawProtocol::new(u32::from(0x0003u16.to_be())).map(rustix::net::Protocol::from_raw);

    Capabilities {
        transport: can(AddressFamily::INET, SocketType::DGRAM, None),
        network: can(AddressFamily::INET, SocketType::RAW, ip_raw),
        link: can(AddressFamily::PACKET, SocketType::RAW, eth_all),
    }
}

/// Stub when there's no native backend (non-Linux, or no socket feature enabled):
/// nothing is openable (use [`Loopback`](crate::Loopback) for tests).
#[cfg(not(all(target_os = "linux", feature = "transport")))]
#[must_use]
pub fn capabilities() -> Capabilities {
    Capabilities {
        link: false,
        network: false,
        transport: false,
    }
}
