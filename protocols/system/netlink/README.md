# rsl-netlink

Strict Linux Netlink message codecs and a blocking `rustix` transport. The crate implements the
route-netlink and generic-netlink operations needed by `nesos`, including sequence/ACK/multipart
validation, extended acknowledgements, typed network configuration, and WireGuard device state.

Netlink is a Linux kernel/userspace socket protocol rather than an OSI network layer, so the crate
lives under `protocols/system`. Its package and Rust import names remain `rsl-netlink` and
`rsl_netlink`.

The controlling wire definitions are the Linux UAPI headers and the
[Linux kernel Netlink documentation](https://www.kernel.org/doc/html/latest/userspace-api/netlink/).

The API is experimental before 1.0.
