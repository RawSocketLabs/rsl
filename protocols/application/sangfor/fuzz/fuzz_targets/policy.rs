//! The policy a gateway hands out (spec §6), read from bytes the gateway chose: reading it
//! and every lookup against it must never panic, a document cannot yield more rules than it
//! has bytes, and an address block or range becomes the one rule the specification gives it.
//!
//! The input is read three ways. As a whole document. As the text values of a single rule
//! (host, ports, protocol, pinned address, node address, then a name to look up, one per
//! line) set into a valid document. And as numbers spelling an address block or range, a port
//! range and a pinned address in the same document. The value parsers and the lookups are
//! then reached without rediscovering the document's shape or the syntax of an address.
//!
//! Run: `cargo +nightly fuzz run policy --fuzz-dir protocols/application/sangfor/fuzz`.
#![no_main]

use std::fmt::Write;
use std::net::Ipv4Addr;

use libfuzzer_sys::fuzz_target;
use sangfor_atrust::policy::{Policy, Transport};
use sangfor_atrust::tcp::Destination;

const GATEWAY: &str = "sdp.example.org";

/// `text` as the inside of a JSON string.
fn escaped(text: &str) -> String {
    let mut inside = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '"' => inside.push_str("\\\""),
            '\\' => inside.push_str("\\\\"),
            control if control < ' ' => {
                write!(inside, "\\u{:04x}", u32::from(control)).unwrap();
            }
            other => inside.push(other),
        }
    }
    inside
}

/// Read `document`, which must be JSON, and hold the policy to its size bound.
fn read(document: &[u8]) -> Option<Policy> {
    let policy = Policy::parse(document, GATEWAY).ok()?;
    let rules = policy.ip_rules.len() + policy.domain_rules.len() + policy.pins.len();
    assert!(
        rules <= document.len(),
        "a document yielded more rules than it has bytes"
    );
    Some(policy)
}

/// A valid document whose one rule has the given values, each the inside of a JSON string.
fn document(flags: u8, protocol: &str, port: &str, host: &str, pin: &str, node: &str) -> String {
    format!(
        concat!(
            r#"{{"data":{{"appList":{{"data":{{"appInfo":[{{"apps":[{{"id":"a","nodeGroupId":"g","#,
            r#""accessModel":"L3VPN","enableTCPPrefL3":{},"addrPretend":{},"addressList":[{{"#,
            r#""protocol":"{}","port":"{}","host":"{}","ip":["{}"]}}]}}]}}],"config":{{"#,
            r#""nodeGroupConf":{{"majorNodeGroup":{{"id":"g"}},"nodeGroupList":[{{"id":"g","#,
            r#""addressInfo":[{{"address":"{}","type":"WAN"}}]}}]}}}}}}}}}}}}"#,
        ),
        flags & 1 != 0,
        flags & 2 != 0,
        protocol,
        port,
        host,
        pin,
        node,
    )
}

/// Every lookup a client makes, for `name` and for `address`.
fn look_up(policy: &Policy, name: &str, address: Ipv4Addr, port: u16, take_all: bool) {
    for transport in [Transport::Tcp, Transport::Udp, Transport::Icmp] {
        let _ = policy.domain_rule(name, transport, port);
        let _ = policy.ip_matches(address, transport, port).count();
    }
    let _ = policy.domain_candidates(name);
    let _ = policy.pinned(name).count();
    let _ = policy.tcp_rule_for_name(name, port, take_all);
    let _ = policy.tcp_rule_for_address(address, port, take_all);
    let _ = Destination::route(policy, name, port, take_all, |_| Some(address));
    let _ = Destination::route(policy, &address.to_string(), port, take_all, |_| None);
    let _ = policy.node("g");
    let _ = policy.node("");
}

fuzz_target!(|data: &[u8]| {
    let _ = read(data);

    let Some((&selector, values)) = data.split_first() else {
        return;
    };
    let take_all = selector & 4 != 0;
    let port = [443, 80, 0, u16::MAX][usize::from(selector >> 3 & 3)];
    let address = Ipv4Addr::new(198, 51, 100, selector);

    // The values as text, one per line.
    let text = String::from_utf8_lossy(values);
    let lines: Vec<&str> = text.split('\n').collect();
    let line = |index: usize| lines.get(index).copied().unwrap_or_default();
    let free = document(
        selector,
        &escaped(line(2)),
        &escaped(line(1)),
        &escaped(line(0)),
        &escaped(line(3)),
        &escaped(line(4)),
    );
    // Whatever a rule's values are, the document is JSON, and JSON is all a policy needs.
    let policy = read(free.as_bytes()).expect("the document is JSON whatever fills it");
    // The rule's own host is the name most likely to be selected.
    for name in [line(5), line(0)] {
        look_up(&policy, name, address, port, take_all);
    }

    // The values as numbers: an address block or range, a port range and a pinned address,
    // which text alone seldom spells.
    let byte = |index: usize| values.get(index).copied().unwrap_or_default();
    let quad = |at: usize| Ipv4Addr::new(byte(at), byte(at + 1), byte(at + 2), byte(at + 3));
    let host = if selector & 32 == 0 {
        format!("{}/{}", quad(0), byte(4) % 40)
    } else {
        format!("{}-{}", quad(0), quad(4))
    };
    let ports = format!(
        "{}-{}",
        u16::from_be_bytes([byte(8), byte(9)]),
        u16::from_be_bytes([byte(10), byte(11)])
    );
    let numeric = document(selector, "all", &ports, &host, &quad(12).to_string(), "n:1");
    let policy = read(numeric.as_bytes()).expect("the document is JSON whatever fills it");
    // §6.2: a block of 2^(32 - n) addresses runs from its network address to its broadcast
    // address; a range runs between its bounds as written; either is the document's one rule.
    let bounds = if selector & 32 != 0 {
        Some((quad(0), quad(4)))
    } else {
        let prefix = u32::from(byte(4) % 40);
        (prefix <= 32).then(|| {
            let size = 1_u64 << (32 - prefix);
            let network = u64::from(u32::from(quad(0))) / size * size;
            let at = |address: u64| Ipv4Addr::from(u32::try_from(address).unwrap());
            (at(network), at(network + size - 1))
        })
    };
    if let Some(bounds) = bounds {
        let rules: Vec<_> = policy
            .ip_rules
            .iter()
            .map(|rule| (rule.min, rule.max))
            .collect();
        assert_eq!(rules, [bounds], "{host} is not the one rule it spells");
    }
    look_up(&policy, "app.example.net", quad(0), port, take_all);
    look_up(&policy, "app.example.net", quad(4), port, take_all);
});
