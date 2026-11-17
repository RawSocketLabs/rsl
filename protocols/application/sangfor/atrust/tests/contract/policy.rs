//! The policy model, checked against the vectors of `../../../SPEC-atrust.md` §12.

use std::net::Ipv4Addr;

use sangfor_atrust::policy::{NameRoute, Policy, Ports, Protocol, Transport};

use crate::support::fixtures::V21;

fn v21() -> Policy {
    Policy::parse(V21.as_bytes(), "sdp.example.org").unwrap()
}

/// A policy of `L3VPN` applications, each given as (id, extra members, address list).
fn policy_of(apps: &[(&str, &str, &str)]) -> Policy {
    let apps: Vec<String> = apps
        .iter()
        .map(|(id, extra, addresses)| {
            format!(r#"{{"id":"{id}","accessModel":"L3VPN"{extra},"addressList":[{addresses}]}}"#)
        })
        .collect();
    let document = format!(
        r#"{{"data":{{"appList":{{"data":{{"appInfo":[{{"apps":[{}]}}]}}}}}}}}"#,
        apps.join(",")
    );
    Policy::parse(document.as_bytes(), "sdp.example.org").unwrap()
}

/// One address-list entry.
fn address(protocol: &str, port: &str, host: &str) -> String {
    format!(r#"{{"protocol":"{protocol}","port":"{port}","host":"{host}"}}"#)
}

fn ip(text: &str) -> Ipv4Addr {
    text.parse().unwrap()
}

#[test]
fn the_v21_document_yields_its_rules() {
    let policy = v21();

    // The range, then the rule the pin makes. app-0003 is not L3VPN and yields nothing.
    assert_eq!(policy.ip_rules.len(), 2);
    let range = &policy.ip_rules[0];
    assert_eq!(
        (range.min, range.max),
        (ip("198.51.100.0"), ip("198.51.100.255"))
    );
    assert_eq!(range.protocol, Protocol::All);
    assert!(range.ports.contains(1) && range.ports.contains(65535) && !range.ports.contains(0));
    assert_eq!(range.target.app_id, "app-0001");
    assert_eq!(range.target.node_group_id, "ng-1");
    assert!(range.target.tcp_prefers_l3);

    let pinned = &policy.ip_rules[1];
    assert_eq!(
        (pinned.min, pinned.max),
        (ip("203.0.113.5"), ip("203.0.113.5"))
    );
    assert_eq!(pinned.protocol, Protocol::Tcp);
    assert!(pinned.ports.contains(443) && !pinned.ports.contains(444));
    assert_eq!(pinned.target.app_id, "app-0002");

    assert_eq!(policy.domain_rules.len(), 2);
    let exact = &policy.domain_rules[0];
    assert_eq!(
        (exact.name.as_str(), exact.protocol),
        ("app.example.net", Protocol::Tcp)
    );
    assert_eq!(exact.target.node_group_id, "ng-2");
    assert!(!exact.target.addr_pretend && !exact.target.tcp_prefers_l3);
    let suffix = &policy.domain_rules[1];
    assert_eq!(
        (suffix.name.as_str(), suffix.protocol),
        (".example.net", Protocol::Udp)
    );
    assert!(
        suffix.ports.contains(5000) && suffix.ports.contains(5010) && !suffix.ports.contains(5011)
    );

    let pins: Vec<(&str, Ipv4Addr)> = policy
        .pins
        .iter()
        .map(|(host, ip)| (&**host, *ip))
        .collect();
    assert_eq!(pins, [("app.example.net", ip("203.0.113.5"))]);
}

#[test]
fn the_v21_document_yields_its_nodes_and_dns_servers() {
    let policy = v21();
    assert_eq!(policy.major_group, "ng-1");
    assert_eq!(policy.node_groups.len(), 2);
    assert_eq!(policy.node_groups[0].wan, ["sdp.example.org:441"]);
    assert_eq!(policy.node_groups[0].lan, ["198.51.100.200:4441"]);
    assert_eq!(policy.node_groups[1].wan, ["node2.example.org:441"]);
    // dnsOption's first server is empty, so dnsOptionV2 is used.
    assert_eq!(policy.dns_servers, ["192.0.2.53", "192.0.2.54"]);
}

// §6.3.
#[test]
fn dns_option_wins_when_its_first_server_is_set() {
    let policy = Policy::parse(
        br#"{"data":{"sdpPolicy":{"data":{"clientOption":{
            "dnsOption":{"firstDNS":"192.0.2.1","secondDNS":""},
            "dnsOptionV2":{"firstDNS":"192.0.2.53","secondDNS":"192.0.2.54"}}}}}}"#,
        "sdp.example.org",
    )
    .unwrap();
    assert_eq!(policy.dns_servers, ["192.0.2.1"]);
}

// §6.1, §2.5.
#[test]
fn keys_are_matched_without_regard_to_letter_case() {
    let policy = Policy::parse(
        br#"{"DATA":{"AppList":{"Data":{
          "AppInfo":[{"Apps":[{"ID":"app-1","NodeGroupID":"ng-9","AccessModel":"L3VPN",
            "EnableTcpPrefL3":true,"AddrPretend":false,
            "AddressList":[{"Protocol":"tcp","Port":"443","Host":"a.example.org","IP":["192.0.2.9"]}]}]}],
          "Config":{"NodeGroupConf":{"MajorNodeGroup":{"ID":"ng-9"},
            "NodeGroupList":[{"ID":"ng-9","AddressInfo":[{"Address":"n.example.org","Type":"Wan"}]}]}}}}}}"#,
        "sdp.example.org",
    )
    .unwrap();
    let rule = &policy.domain_rules[0];
    assert_eq!(rule.target.app_id, "app-1");
    assert_eq!(rule.target.node_group_id, "ng-9");
    assert!(rule.target.tcp_prefers_l3 && !rule.target.addr_pretend);
    assert_eq!(policy.pins.len(), 1);
    assert_eq!(policy.node("ng-9"), Some("n.example.org:441"));
}

// §6.2.
#[test]
fn addr_pretend_is_false_only_for_the_boolean_false() {
    let entry = address("tcp", "443", "a.example.org");
    let policy = policy_of(&[
        ("absent", "", &entry),
        ("true", r#","addrPretend":true"#, &entry),
        ("false", r#","addrPretend":false"#, &entry),
        ("text", r#","addrPretend":"false""#, &entry),
        ("zero", r#","addrPretend":0"#, &entry),
    ]);
    let pretend: Vec<(&str, bool)> = policy
        .domain_rules
        .iter()
        .map(|rule| (rule.target.app_id.as_str(), rule.target.addr_pretend))
        .collect();
    assert_eq!(
        pretend,
        [
            ("absent", true),
            ("true", true),
            ("false", false),
            ("text", true),
            ("zero", true)
        ]
    );
}

#[test]
fn domain_selection_matches_v23() {
    let policy = policy_of(&[
        ("app-a", "", &address("tcp", "443", "example.org")),
        ("app-b", "", &address("tcp", "1-65535", "*.example.org")),
        (
            "app-c",
            "",
            &address("tcp", "8000-8999", "*.DEV.example.Org."),
        ),
        ("app-d", "", &address("udp", "53", "www.example.org")),
    ]);
    let names: Vec<&str> = policy
        .domain_rules
        .iter()
        .map(|rule| rule.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "example.org",
            ".example.org",
            ".dev.example.org",
            "www.example.org"
        ]
    );

    let candidates = |name: &str| -> Vec<&str> {
        policy
            .domain_candidates(name)
            .iter()
            .map(|rule| rule.name.as_str())
            .collect()
    };
    assert_eq!(candidates("example.org"), ["example.org"]);
    assert_eq!(
        candidates("www.example.org"),
        ["www.example.org", ".example.org", "example.org"]
    );
    assert_eq!(
        candidates("Www.EXAMPLE.org."),
        candidates("www.example.org")
    );
    assert_eq!(
        candidates("api.dev.example.org"),
        [".dev.example.org", ".example.org", "example.org"]
    );
    assert_eq!(
        candidates("dev.example.org"),
        [".example.org", "example.org"]
    );
    assert_eq!(candidates("badexample.org").len(), 0);

    let rows = [
        ("example.org", Transport::Tcp, 443, Some("app-a")),
        ("example.org", Transport::Tcp, 80, None),
        ("www.example.org", Transport::Tcp, 443, Some("app-b")),
        ("Www.EXAMPLE.org.", Transport::Tcp, 443, Some("app-b")),
        ("www.example.org", Transport::Udp, 53, Some("app-d")),
        ("api.dev.example.org", Transport::Tcp, 8080, Some("app-c")),
        ("api.dev.example.org", Transport::Tcp, 443, Some("app-b")),
        ("dev.example.org", Transport::Tcp, 8080, Some("app-b")),
        ("badexample.org", Transport::Tcp, 443, None),
    ];
    for (name, transport, port, expected) in rows {
        let found = policy
            .domain_rule(name, transport, port)
            .map(|rule| rule.target.app_id.as_str());
        assert_eq!(found, expected, "{name} {transport:?} {port}");
    }
}

#[test]
fn port_strings_match_v24() {
    let ports_of = |port: &str| {
        policy_of(&[("app", "", &address("tcp", port, "192.0.2.7"))])
            .ip_rules
            .first()
            .map(|rule| rule.ports)
    };
    let contains = |port: &str, probe: u16| ports_of(port).unwrap().contains(probe);

    assert!(contains("443", 443) && !contains("443", 442) && !contains("443", 444));
    assert!(contains("8000-8100", 8000) && contains("8000-8100", 8100));
    assert!(!contains("8000-8100", 7999) && !contains("8000-8100", 8101));
    for malformed in ["", " 443", "80-", "-80", "1-2-3", "+80", "4 43", "0x50"] {
        assert_eq!(
            ports_of(malformed),
            None,
            "port {malformed:?} must skip the address"
        );
    }
    // Kept, but no port is in it.
    assert_eq!(ports_of("9000-8000"), Some(Ports::EMPTY));
    assert!(!contains("70000", 65535) && !contains("70000", 4464));
    assert!(contains("0", 0) && !contains("0", 1));
    assert!(contains("1-70000", 1) && contains("1-70000", 65535) && !contains("1-70000", 0));
}

// §6.2, port strings.
#[test]
fn a_rule_with_no_port_in_range_still_matches_icmp_under_all() {
    let policy = policy_of(&[("app", "", &address("all", "9000-8000", "192.0.2.7"))]);
    assert_eq!(
        policy
            .ip_matches(ip("192.0.2.7"), Transport::Icmp, 0)
            .count(),
        1
    );
    assert_eq!(
        policy
            .ip_matches(ip("192.0.2.7"), Transport::Tcp, 8500)
            .count(),
        0
    );
}

#[test]
fn host_strings_match_v24() {
    let range_of = |host: &str| {
        policy_of(&[("app", "", &address("tcp", "443", host))])
            .ip_rules
            .first()
            .map(|rule| (rule.min, rule.max))
    };
    let rows = [
        ("192.0.2.7", "192.0.2.7", "192.0.2.7"),
        ("198.51.100.77/24", "198.51.100.0", "198.51.100.255"),
        ("203.0.113.9/32", "203.0.113.9", "203.0.113.9"),
        ("0.0.0.0/0", "0.0.0.0", "255.255.255.255"),
        ("192.0.2.16-192.0.2.31", "192.0.2.16", "192.0.2.31"),
        ("::ffff:192.0.2.7", "192.0.2.7", "192.0.2.7"),
        // Where this implementation differs from the reference, as the specification
        // allows: white space around a host is removed before it is classified.
        (" 192.0.2.7", "192.0.2.7", "192.0.2.7"),
    ];
    for (host, min, max) in rows {
        assert_eq!(range_of(host), Some((ip(min), ip(max))), "host {host:?}");
    }

    // A reversed range keeps its place and matches nothing.
    let reversed = policy_of(&[("app", "", &address("tcp", "443", "192.0.2.31-192.0.2.16"))]);
    for probe in ["192.0.2.16", "192.0.2.20", "192.0.2.31"] {
        assert_eq!(
            reversed.ip_matches(ip(probe), Transport::Tcp, 443).count(),
            0
        );
    }

    for skipped in [
        "2001:db8::24",
        "2001:db8::/32",
        "2001:db8::1-2001:db8::2",
        "a*.example.org",
        "*",
    ] {
        let policy = policy_of(&[("app", "", &address("tcp", "443", skipped))]);
        assert!(
            policy.ip_rules.is_empty() && policy.domain_rules.is_empty(),
            "host {skipped:?}"
        );
    }

    let names = |host: &str| -> Vec<String> {
        policy_of(&[("app", "", &address("tcp", "443", host))])
            .domain_rules
            .into_iter()
            .map(|rule| rule.name)
            .collect()
    };
    assert_eq!(names("*.example.org"), [".example.org"]);
    // Not addresses, so names: a hyphenated name, and a prefix length no block has.
    assert_eq!(names("my-host.example.org"), ["my-host.example.org"]);
    assert_eq!(names("192.0.2.7/33"), ["192.0.2.7/33"]);
}

// §6.2, value spelling; V24.
#[test]
fn value_spelling_is_exact() {
    let upper_protocol = policy_of(&[("app", "", &address("TCP", "443", "192.0.2.7"))]);
    assert_eq!(upper_protocol.ip_rules.len(), 0);

    let lower_model = Policy::parse(
        br#"{"data":{"appList":{"data":{"appInfo":[{"apps":[{"id":"app","accessModel":"l3vpn",
            "addressList":[{"protocol":"tcp","port":"443","host":"192.0.2.7"}]}]}]}}}}"#,
        "sdp.example.org",
    )
    .unwrap();
    assert_eq!(lower_model.ip_rules.len(), 0);
}

#[test]
fn rule_order_with_pinned_addresses_matches_v25() {
    let pinned = r#"{"protocol":"tcp","port":"443","host":"db.example.net",
        "ip":["203.0.113.20","2001:db8::20","203.0.113.21"]}"#;
    let policy = policy_of(&[
        ("app-0010", "", &address("tcp", "1-1023", "203.0.113.0/24")),
        ("app-0011", "", pinned),
        ("app-0012", "", &address("tcp", "443", "203.0.113.21")),
    ]);

    let sequence: Vec<(Ipv4Addr, &str)> = policy
        .ip_rules
        .iter()
        .map(|rule| (rule.min, rule.target.app_id.as_str()))
        .collect();
    assert_eq!(
        sequence,
        [
            (ip("203.0.113.0"), "app-0010"),
            (ip("203.0.113.20"), "app-0011"),
            (ip("203.0.113.21"), "app-0011"),
            (ip("203.0.113.21"), "app-0012"),
        ]
    );
    let pins: Vec<Ipv4Addr> = policy.pinned("DB.example.net.").collect();
    assert_eq!(pins, [ip("203.0.113.20"), ip("203.0.113.21")]);

    let rows = [
        ("203.0.113.20", 443, "app-0010", "app-0011"),
        ("203.0.113.21", 443, "app-0010", "app-0012"),
        ("203.0.113.21", 80, "app-0010", "app-0010"),
        ("203.0.113.99", 443, "app-0010", "app-0010"),
    ];
    for (address, port, first, last) in rows {
        let mut matches = policy.ip_matches(ip(address), Transport::Tcp, port);
        let found_first = matches.next().unwrap().target.app_id.clone();
        let found_last = matches
            .next_back()
            .map_or(found_first.clone(), |rule| rule.target.app_id.clone());
        assert_eq!(
            (found_first.as_str(), found_last.as_str()),
            (first, last),
            "{address}:{port}"
        );
    }
}

// §6.2, where pinned addresses sit.
#[test]
fn a_wildcard_host_with_pinned_addresses_yields_rules_but_no_pin() {
    let policy = policy_of(&[(
        "app",
        "",
        r#"{"protocol":"tcp","port":"443","host":"*.example.org","ip":["192.0.2.9"]}"#,
    )]);
    assert_eq!(policy.ip_rules.len(), 1);
    assert_eq!(policy.pins.len(), 0);
}

#[test]
fn tcp_tunnel_rule_choice_matches_v26() {
    let policy = v21();

    // A name rule that leaves TCP to this tunnel; the name is normalised.
    for name in ["app.example.net", "APP.Example.NET.", "www.app.example.net"] {
        let NameRoute::Rule(rule) = policy.tcp_rule_for_name(name, 443, false) else {
            panic!("{name} must be carried by a name rule");
        };
        assert_eq!(
            (
                rule.target.app_id.as_str(),
                rule.target.node_group_id.as_str()
            ),
            ("app-0002", "ng-2")
        );
        assert!(!rule.target.addr_pretend);
    }
    assert_eq!(
        policy.pinned("app.example.net").next(),
        Some(ip("203.0.113.5"))
    );
    assert_eq!(policy.pinned("www.app.example.net").next(), None);

    // The address the pin made reachable.
    let rule = policy
        .tcp_rule_for_address(ip("203.0.113.5"), 443, false)
        .unwrap();
    assert_eq!(rule.target.app_id, "app-0002");

    // The only candidate for this name is the udp rule.
    assert_eq!(
        policy.tcp_rule_for_name("x.example.net", 443, false),
        NameRoute::Unmatched
    );
    assert!(
        policy
            .tcp_rule_for_address(ip("203.0.113.77"), 443, true)
            .is_none()
    );

    // The one matching rule prefers L3: taken only by a client with no L3 tunnel.
    assert!(
        policy
            .tcp_rule_for_address(ip("198.51.100.9"), 22, false)
            .is_none()
    );
    let rule = policy
        .tcp_rule_for_address(ip("198.51.100.9"), 22, true)
        .unwrap();
    assert_eq!(
        (
            rule.target.app_id.as_str(),
            rule.target.node_group_id.as_str()
        ),
        ("app-0001", "ng-1")
    );

    // A node's own address is looked up like any other.
    let rule = policy
        .tcp_rule_for_address(ip("198.51.100.200"), 4441, true)
        .unwrap();
    assert_eq!(rule.target.app_id, "app-0001");
}

// §6.6 step 5.1.
#[test]
fn a_name_rule_that_prefers_l3_yields_to_one_that_does_not() {
    let entry = address("tcp", "443", "a.example.org");
    let l3 = r#","enableTCPPrefL3":true"#;

    let only_l3 = policy_of(&[("l3", l3, &entry)]);
    assert_eq!(
        only_l3.tcp_rule_for_name("a.example.org", 443, false),
        NameRoute::PrefersL3
    );
    assert!(matches!(
        only_l3.tcp_rule_for_name("a.example.org", 443, true),
        NameRoute::Rule(rule) if rule.target.app_id == "l3"
    ));

    // The first rule that leaves TCP to this tunnel wins, wherever it stands.
    let mixed = policy_of(&[
        ("l3", l3, &entry),
        ("tcp-1", "", &entry),
        ("tcp-2", "", &entry),
    ]);
    for take_all in [false, true] {
        assert!(matches!(
            mixed.tcp_rule_for_name("a.example.org", 443, take_all),
            NameRoute::Rule(rule) if rule.target.app_id == "tcp-1"
        ));
    }
}

// §6.6 step 5.2.
#[test]
fn the_last_address_rule_that_leaves_tcp_to_this_tunnel_wins() {
    let entry = address("tcp", "443", "192.0.2.7");
    let l3 = r#","enableTCPPrefL3":true"#;
    let policy = policy_of(&[
        ("tcp-1", "", &entry),
        ("tcp-2", "", &entry),
        ("l3", l3, &entry),
    ]);
    for take_all in [false, true] {
        let rule = policy
            .tcp_rule_for_address(ip("192.0.2.7"), 443, take_all)
            .unwrap();
        assert_eq!(rule.target.app_id, "tcp-2", "take_all = {take_all}");
    }
    assert!(
        policy
            .tcp_rule_for_address(ip("192.0.2.7"), 80, true)
            .is_none()
    );
}

// §6.6 step 6.
#[test]
fn a_node_comes_from_the_rules_group_then_the_major_group() {
    let policy = v21();
    assert_eq!(policy.node("ng-2"), Some("node2.example.org:441"));
    assert_eq!(policy.node("ng-1"), Some("sdp.example.org:441"));
    // Unknown and empty ids lead to the major group.
    assert_eq!(policy.node("ng-404"), Some("sdp.example.org:441"));
    assert_eq!(policy.node(""), Some("sdp.example.org:441"));

    // No outside node: the first inside one. No node anywhere: none.
    let lan_only = Policy::parse(
        br#"{"data":{"appList":{"data":{"config":{"nodeGroupConf":{"majorNodeGroup":{"id":"m"},
            "nodeGroupList":[{"id":"m","addressInfo":[{"address":"192.0.2.1","type":"LAN"},
              {"address":"192.0.2.2","type":"other"}]},{"id":"empty","addressInfo":[]}]}}}}}}"#,
        "sdp.example.org",
    )
    .unwrap();
    assert_eq!(lan_only.node("empty"), Some("192.0.2.1:441"));
    assert_eq!(Policy::default().node("m"), None);
}

// §6.4.
#[test]
fn the_server_placeholder_is_replaced_only_as_a_whole_address() {
    let policy = Policy::parse(
        br#"{"data":{"appList":{"data":{"config":{"nodeGroupConf":{"nodeGroupList":[{"id":"g",
            "addressInfo":[{"address":"{{sdpcHost}}","type":"wan"},
              {"address":"{{sdpcHost}}:8443","type":"wan"}]}]}}}}}}"#,
        "sdp.example.org",
    )
    .unwrap();
    assert_eq!(
        policy.node_groups[0].wan,
        ["sdp.example.org:441", "{{sdpcHost}}:8443"]
    );
}

// §6.5: the protocol test, for address rules and for name rules.
#[test]
fn a_rule_applies_only_to_its_own_protocol_and_icmp_needs_all() {
    let policy = policy_of(&[
        ("tcp", "", &address("tcp", "53", "192.0.2.1")),
        ("udp", "", &address("udp", "53", "192.0.2.2")),
        ("all", "", &address("all", "53", "192.0.2.3")),
        ("tcp-name", "", &address("tcp", "53", "t.example.org")),
        ("all-name", "", &address("all", "53", "a.example.org")),
    ]);
    let count = |address: &str, transport| policy.ip_matches(ip(address), transport, 53).count();
    let rows = [
        ("192.0.2.1", [1, 0, 0]),
        ("192.0.2.2", [0, 1, 0]),
        ("192.0.2.3", [1, 1, 1]),
    ];
    for (address, expected) in rows {
        let found = [Transport::Tcp, Transport::Udp, Transport::Icmp]
            .map(|transport| count(address, transport));
        assert_eq!(found, expected, "{address}: tcp, udp, icmp");
    }

    assert!(
        policy
            .domain_rule("t.example.org", Transport::Udp, 53)
            .is_none()
    );
    assert!(
        policy
            .domain_rule("t.example.org", Transport::Icmp, 53)
            .is_none()
    );
    // ICMP has no ports: the rule's range is not consulted.
    assert!(
        policy
            .domain_rule("a.example.org", Transport::Icmp, 0)
            .is_some()
    );
    assert!(
        policy
            .domain_rule("a.example.org", Transport::Udp, 0)
            .is_none()
    );
}

// §6.5, normalisation: both sides, white space included.
#[test]
fn names_are_normalised_on_both_sides() {
    let policy = policy_of(&[(
        "app",
        "",
        r#"{"protocol":"tcp","port":"443","host":"  DB.Example.NET. ","ip":["192.0.2.9"]}"#,
    )]);
    assert_eq!(policy.domain_rules[0].name, "db.example.net");
    assert!(
        policy
            .domain_rule(" db.EXAMPLE.net.\t", Transport::Tcp, 443)
            .is_some()
    );
    // The pin is filed under the normalised name (a permitted difference from the reference).
    assert_eq!(&*policy.pins[0].0, "db.example.net");
    assert_eq!(
        policy.pinned("db.example.net").next(),
        Some(ip("192.0.2.9"))
    );
    // Only one trailing dot is removed.
    assert!(
        policy
            .domain_rule("db.example.net..", Transport::Tcp, 443)
            .is_none()
    );
}

// §6.6 step 5.1: the preference for a rule that leaves TCP to this tunnel reaches past the
// most specific name.
#[test]
fn a_less_specific_name_that_leaves_tcp_here_beats_a_more_specific_one_that_does_not() {
    let policy = policy_of(&[
        (
            "l3",
            r#","enableTCPPrefL3":true"#,
            &address("tcp", "443", "www.example.org"),
        ),
        ("tcp", "", &address("tcp", "443", "example.org")),
    ]);
    assert!(matches!(
        policy.tcp_rule_for_name("www.example.org", 443, false),
        NameRoute::Rule(rule) if rule.target.app_id == "tcp"
    ));
}

// §6.5: a rule with no name is ignored; §6.2 withholds pinned-address rules only from a
// host dropped for its `*`.
#[test]
fn a_host_with_no_name_makes_no_name_rule_but_keeps_its_pinned_addresses() {
    for host in ["", ".", " "] {
        let entry =
            format!(r#"{{"protocol":"tcp","port":"443","host":"{host}","ip":["192.0.2.9"]}}"#);
        let policy = policy_of(&[("app", "", &entry)]);
        assert_eq!(policy.domain_rules.len(), 0, "host {host:?}");
        assert_eq!(policy.pins.len(), 0, "host {host:?}");
        assert_eq!(
            policy
                .ip_matches(ip("192.0.2.9"), Transport::Tcp, 443)
                .count(),
            1,
            "host {host:?}"
        );
    }
    let dropped = policy_of(&[(
        "app",
        "",
        r#"{"protocol":"tcp","port":"443","host":"a*.example.org","ip":["192.0.2.9"]}"#,
    )]);
    assert_eq!(dropped.ip_rules.len(), 0);
}

// §6.6 step 6: an empty id has no record, so it leads to the major group.
#[test]
fn an_empty_group_id_never_names_a_group() {
    let policy = Policy::parse(
        br#"{"data":{"appList":{"data":{"config":{"nodeGroupConf":{"majorNodeGroup":{"id":"m"},
            "nodeGroupList":[{"id":"","addressInfo":[{"address":"noid.example.org","type":"wan"}]},
              {"id":"m","addressInfo":[{"address":"major.example.org","type":"wan"}]}]}}}}}}"#,
        "sdp.example.org",
    )
    .unwrap();
    assert_eq!(policy.node(""), Some("major.example.org:441"));
}

// §6.2, port strings: numbers are not range-checked, however large.
#[test]
fn port_numbers_too_large_to_hold_are_kept_as_past_every_port() {
    let huge = "99999999999999999999999";
    let lower = policy_of(&[("app", "", &address("all", huge, "192.0.2.7"))]);
    assert_eq!(lower.ip_rules[0].ports, Ports::EMPTY);
    assert_eq!(
        lower
            .ip_matches(ip("192.0.2.7"), Transport::Icmp, 0)
            .count(),
        1
    );

    let upper = policy_of(&[(
        "app",
        "",
        &address("tcp", &format!("1-{huge}"), "192.0.2.7"),
    )]);
    assert_eq!(upper.ip_rules[0].ports, Ports::new(1, 65535));
}
