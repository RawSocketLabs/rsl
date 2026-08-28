# rsl-netlink

Runtime-neutral Linux netlink message codecs plus an optional Tokio transport.
The crate implements the route-netlink and generic-netlink operations needed by
`nesos`, including strict response validation and WireGuard device state.

The API is experimental before 1.0.
