//! Policy documents a gateway might send that are wrong, odd or hostile.

use std::sync::Arc;

use sangfor_atrust::policy::{Error, Policy};

fn parse(document: &str) -> Result<Policy, Error> {
    Policy::parse(document.as_bytes(), "sdp.example.org")
}

#[test]
fn text_that_is_not_json_is_an_error() {
    for document in ["", "{", "<html>", "{\"data\":"] {
        assert!(
            matches!(parse(document), Err(Error::Json(_))),
            "{document:?}"
        );
    }
}

#[test]
fn json_without_a_policy_in_it_is_an_empty_policy() {
    for document in [
        "{}",
        "[]",
        "null",
        "7",
        "\"data\"",
        r#"{"data":null}"#,
        r#"{"code":10000004}"#,
    ] {
        assert_eq!(parse(document).unwrap(), Policy::default(), "{document}");
    }
}

#[test]
fn members_of_the_wrong_type_yield_nothing_and_do_not_panic() {
    let documents = [
        r#"{"data":{"appList":{"data":{"appInfo":{"apps":[]}}}}}"#,
        r#"{"data":{"appList":{"data":{"appInfo":[7,null,"x",{"apps":7},{"apps":[7,null,[]]}]}}}}"#,
        r#"{"data":{"appList":{"data":{"appInfo":[{"apps":[{"id":7,"accessModel":"L3VPN",
            "addressList":[{"protocol":"tcp","port":"443","host":"192.0.2.7"}]}]}]}}}}"#,
        r#"{"data":{"appList":{"data":{"appInfo":[{"apps":[{"id":"a","accessModel":"L3VPN",
            "addressList":[7,null,{"protocol":7,"port":443,"host":["192.0.2.7"]},
              {"protocol":"tcp","port":443,"host":"192.0.2.7"},
              {"protocol":"tcp","port":"443","host":7}]}]}]}}}}"#,
        r#"{"data":{"appList":{"data":{"config":{"nodeGroupConf":{"majorNodeGroup":"ng",
            "nodeGroupList":[7,{"id":7,"addressInfo":[7,{"address":7,"type":"wan"},
              {"address":"","type":"wan"},{"address":"n.example.org","type":7}]}]}}}}}}"#,
        r#"{"data":{"sdpPolicy":{"data":{"clientOption":{"dnsOption":7,"dnsOptionV2":[1]}}}}}"#,
    ];
    for document in documents {
        let policy = parse(document).unwrap();
        assert!(
            policy.ip_rules.is_empty() && policy.domain_rules.is_empty(),
            "{document}"
        );
        assert!(
            policy.pins.is_empty() && policy.dns_servers.is_empty(),
            "{document}"
        );
        assert!(
            policy
                .node_groups
                .iter()
                .all(|group| group.first_node().is_none()),
            "{document}"
        );
    }
}

#[test]
fn an_application_with_no_id_is_dropped() {
    // Allowed by the specification: a tunnel request needs an application id.
    let policy = parse(
        r#"{"data":{"appList":{"data":{"appInfo":[{"apps":[
            {"accessModel":"L3VPN","addressList":[{"protocol":"tcp","port":"443","host":"192.0.2.7"}]},
            {"id":"","accessModel":"L3VPN","addressList":[{"protocol":"tcp","port":"443","host":"a.example.org"}]}
        ]}]}}}}"#,
    )
    .unwrap();
    assert!(policy.ip_rules.is_empty() && policy.domain_rules.is_empty());
}

#[test]
fn pinned_addresses_that_are_not_ipv4_are_skipped_one_by_one() {
    let policy = parse(
        r#"{"data":{"appList":{"data":{"appInfo":[{"apps":[{"id":"a","accessModel":"L3VPN",
            "addressList":[{"protocol":"tcp","port":"443","host":"a.example.org",
              "ip":[7,null,"2001:db8::1","not an address","192.0.2.300","192.0.2.9"]}]}]}]}}}}"#,
    )
    .unwrap();
    assert_eq!(policy.ip_rules.len(), 1);
    assert_eq!(policy.pins.len(), 1);
}

#[test]
fn deeply_nested_json_is_refused_not_overflowed() {
    let document = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    assert!(matches!(parse(&document), Err(Error::Json(_))));
}

#[test]
fn a_prefix_length_no_block_has_makes_a_name_not_a_wrapped_block() {
    // 4294967328 is 32 past a 32-bit wrap; the huge one saturates. Neither is a CIDR block.
    let policy = parse(
        r#"{"data":{"appList":{"data":{"appInfo":[{"apps":[{"id":"a","accessModel":"L3VPN",
            "addressList":[
              {"protocol":"tcp","port":"443","host":"192.0.2.9/99999999999999999999999"},
              {"protocol":"tcp","port":"443","host":"192.0.2.9/4294967328"}]}]}]}}}}"#,
    )
    .unwrap();
    assert_eq!(policy.ip_rules.len(), 0);
    assert_eq!(policy.domain_rules.len(), 2);
}

#[test]
fn rules_share_what_a_document_could_make_them_copy() {
    // One long id beside many addresses must not cost their product in memory.
    let policy = parse(
        r#"{"data":{"appList":{"data":{"appInfo":[{"apps":[{"id":"a","accessModel":"L3VPN",
            "addressList":[
              {"protocol":"tcp","port":"443","host":"192.0.2.7"},
              {"protocol":"tcp","port":"443","host":"a.example.org","ip":["192.0.2.8","192.0.2.9"]}]}]}]}}}}"#,
    )
    .unwrap();
    let first = &policy.ip_rules[0].target;
    assert!(
        policy
            .ip_rules
            .iter()
            .all(|rule| Arc::ptr_eq(&rule.target, first))
    );
    assert!(Arc::ptr_eq(&policy.domain_rules[0].target, first));
    assert!(Arc::ptr_eq(&policy.pins[0].0, &policy.pins[1].0));
}
