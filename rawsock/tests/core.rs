//! The dual-use sink, `Loopback`, and capability probing.

use rawsock::{Layer, Loopback, RawIo, capabilities};

#[test]
fn loopback_records_and_replays_verbatim() {
    let mut lo = Loopback::new(Layer::Link);
    assert_eq!(lo.layer(), Layer::Link);

    // send_raw is verbatim — even a malformed/short "frame".
    let a = [0xDE, 0xAD, 0xBE, 0xEF];
    let b = [0x00];
    assert_eq!(lo.send_raw(&a).unwrap(), 4);
    assert_eq!(lo.send_raw(&b).unwrap(), 1);

    assert_eq!(lo.sent(), &[a.to_vec(), b.to_vec()]);
    assert_eq!(lo.last_sent(), Some(b.as_slice()));

    // recv replays in order.
    let mut buf = [0u8; 16];
    let n = lo.recv(&mut buf).unwrap();
    assert_eq!(&buf[..n], &a);
    let n = lo.recv(&mut buf).unwrap();
    assert_eq!(&buf[..n], &b);
    assert!(lo.recv(&mut buf).is_err()); // empty
}

#[test]
fn capabilities_are_self_consistent() {
    let caps = capabilities();
    // Whatever the environment, `allows` must agree with the fields.
    assert_eq!(caps.allows(Layer::Link), caps.link);
    assert_eq!(caps.allows(Layer::Network), caps.network);
    assert_eq!(caps.allows(Layer::Transport), caps.transport);

    // Raw layers imply transport (if you can open a raw socket you can open UDP).
    if caps.link || caps.network {
        assert!(caps.transport, "raw layers should imply transport");
    }
}
