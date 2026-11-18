//! Every control-plane answer parser (spec §4), fed a status and a body a gateway chose, must never
//! panic, and the lists it keeps stay bounded.
//!
//! The first byte picks the status and what the rest is: the whole body, the `data` of an
//! accepted envelope, or one entry of both bounded lists repeated past their bound. The
//! payload parsers and the bound are then reached without rediscovering the envelope.
//!
//! Run: `cargo +nightly fuzz run api --fuzz-dir protocols/application/sangfor/fuzz`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use sangfor_atrust::api::{
    AntiMitmData, AntiMitmReply, AuthConfig, Envelope, Manifest, NextStep, OnlineInfo, PswReply,
};
use sangfor_atrust::login::Step;
use sangfor_fuzz::response;

const STATUSES: [u16; 8] = [200, 200, 200, 200, 401, 403, 500, 302];
/// The most entries an answer's list may hold (`MAX_ENTRIES` in `api/members.rs`).
const MAX_ENTRIES: usize = 256;

fuzz_target!(|data: &[u8]| {
    let Some((&selector, rest)) = data.split_first() else {
        return;
    };
    let status = STATUSES[usize::from(selector & 7)];
    let body = match selector >> 3 & 3 {
        1 => [b"{\"code\":0,\"data\":".as_slice(), rest, b"}"].concat(),
        2 => {
            let entry = &rest[..rest.len().min(64)];
            let entries = vec![entry; MAX_ENTRIES + 1].join(b",".as_slice());
            [
                b"{\"code\":0,\"data\":{\"authServerInfoList\":[".as_slice(),
                &entries,
                b"],\"nextServiceList\":[",
                &entries,
                b"]}}",
            ]
            .concat()
        }
        _ => rest.to_vec(),
    };
    let Some(response) = response(status, &[], &body) else {
        return;
    };

    let _ = Manifest::parse(&response);
    if let Ok(config) = AuthConfig::parse(&response) {
        assert!(config.servers.len() <= MAX_ENTRIES);
        let _ = config.server("auth/psw", "local");
    }
    let _ = AntiMitmReply::parse(&response);
    let _ = PswReply::parse(&response);
    if let Ok(next) = NextStep::parse(&response) {
        assert!(next.next_service_list.len() <= MAX_ENTRIES);
        let _ = Step::resolve(&next);
    }
    let _ = OnlineInfo::parse(&response);
    if let Ok(envelope) = Envelope::of_ok(&response) {
        let _ = envelope.accepted();
    }
    if let Ok(text) = std::str::from_utf8(rest) {
        let _ = AntiMitmData::parse(text);
    }
});
