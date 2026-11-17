//! The signed document that authorises a stream.

// --- Standard library ---
use std::net::Ipv4Addr;

// --- Workspace dependencies ---
use sangfor_atrust_sig::identifiers;
use serde::Serialize;

// --- Internal modules ---
use super::{Destination, Error, Identity};

/// The port whose streams are reported as coming from an SSH client.
const SSH_PORT: u16 = 22;
/// The value the vendor client reports for attributes it does not fill in.
const CLOSED: &str = "TrustAppClosed";

/// The document that asks a node to authorise one stream (spec §9.2). The order of members
/// is the order sent, and the signature covers the bytes sent.
#[derive(Serialize)]
struct AuthRequest<'a> {
    /// The session identifier.
    sid: &'a str,
    /// The application of the chosen rule.
    #[serde(rename = "appId")]
    app_id: &'a str,
    /// `tcp://` and the destination.
    url: String,
    /// The device id.
    #[serde(rename = "deviceId")]
    device_id: &'a str,
    /// The connection id.
    #[serde(rename = "connectionId")]
    connection_id: &'a str,
    /// The fingerprint of the process the stream is reported for.
    #[serde(rename = "procHash")]
    proc_hash: &'a str,
    /// The user's name. Always present, empty or not.
    #[serde(rename = "userName")]
    user_name: &'a str,
    /// Always zero.
    #[serde(rename = "rcAppliedInfo")]
    rc_applied_info: u8,
    /// Always `en-US`.
    lang: &'a str,
    /// The destination, `host:port`.
    #[serde(rename = "destAddr")]
    dest_addr: &'a str,
    /// The address the client resolved the destination to, when it did.
    #[serde(rename = "destIP", skip_serializing_if = "Option::is_none")]
    dest_ip: Option<Ipv4Addr>,
    /// What the client says about where the stream comes from.
    env: Env<'a>,
    /// The signature, empty while the document is being signed.
    #[serde(rename = "xRequestSig")]
    x_request_sig: &'a str,
}

/// The `env` member (spec §8.3): constants, describing a process the vendor client trusts.
#[derive(Serialize)]
struct Env<'a> {
    /// The only member.
    application: Application<'a>,
}

/// The `application` member.
#[derive(Serialize)]
struct Application<'a> {
    /// The only member.
    runtime: Runtime<'a>,
}

/// The `runtime` member.
#[derive(Serialize)]
struct Runtime<'a> {
    /// The process.
    process: Process<'a>,
    /// Always `TRUSTED`.
    process_trusted: &'a str,
}

/// The `process` member.
#[derive(Serialize)]
struct Process<'a> {
    /// The program's name.
    name: &'a str,
    /// Not filled in.
    digital_signature: &'a str,
    /// The operating system.
    platform: &'a str,
    /// The fingerprint, again.
    fingerprint: &'a str,
    /// Not filled in.
    description: &'a str,
    /// The program's path.
    path: &'a str,
    /// Not filled in.
    version: &'a str,
    /// Always `normal`.
    security_env: &'a str,
}

/// The signed document for a stream to `destination` on behalf of `identity` (spec §9.2,
/// §8.4): serialised once with an empty signature, signed, and serialised again with the
/// signature in its place.
pub(super) fn signed(identity: &Identity, destination: &Destination) -> Result<Vec<u8>, Error> {
    // What the reference client reports, by destination port; the values are cosmetic.
    let (name, path) = if destination.wire.port() == SSH_PORT {
        ("ssh", "/usr/bin/ssh")
    } else {
        ("google-chrome-stable", "/usr/bin/google-chrome-stable")
    };
    let fingerprint = identifiers::proc_hash(path)?;
    let document = |signature: &str| {
        let request = AuthRequest {
            sid: identity.sid(),
            app_id: &destination.app_id,
            url: format!("tcp://{}", destination.dest_addr),
            device_id: identity.device_id(),
            connection_id: identity.connection_id(),
            proc_hash: &fingerprint,
            user_name: identity.user_name(),
            rc_applied_info: 0,
            lang: "en-US",
            dest_addr: &destination.dest_addr,
            dest_ip: destination.dest_ip,
            env: Env {
                application: Application {
                    runtime: Runtime {
                        process: Process {
                            name,
                            digital_signature: CLOSED,
                            platform: "Linux",
                            fingerprint: &fingerprint,
                            description: CLOSED,
                            path,
                            version: CLOSED,
                            security_env: "normal",
                        },
                        process_trusted: "TRUSTED",
                    },
                },
            },
            x_request_sig: signature,
        };
        serde_json::to_vec(&request)
            .expect("invariant: a struct of strings and numbers always serialises")
    };
    let signature = identity.sign_key().sign(&document(""))?;
    Ok(document(&signature))
}
