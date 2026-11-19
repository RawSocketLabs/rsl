//! Log in to an aTrust gateway with a password and fetch one page through it.
//!
//! ```text
//! ATRUST_PASSWORD=… cargo run -p sangfor-atrust --features net --example tunnel -- \
//!     --insecure vpn.example.org 443 user01 local intranet.example.org 80
//! ```
//!
//! The arguments are the gateway's host and port, the user name, the login domain, and the
//! host and port to reach behind the gateway. The password comes from the environment so
//! that it stays out of the process list.
//!
//! This is the crate's first milestone end to end. It has not been run against a real
//! gateway: until it has, expect the first run to find something.
//!
//! What a run does that a user should know first:
//!
//! - It verifies no certificate, the gateway's or the node's: these gateways present
//!   self-signed ones. The protocol's own anti-MITM checks are run and their outcome is
//!   printed, but **the password is sent whatever they find**, as the vendor's clients do.
//!   `Options::strict_anti_mitm` stops at a failed check instead.
//! - It presents a new device id and MAC address on every run. A gateway that binds accounts
//!   to devices will see each run as a new device; a real client keeps its `Device`.

use std::error::Error;
use std::io::{self, Read, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{env, process};

use os_random::OsRandom;
use sangfor_atrust::api::Gateway;
use sangfor_atrust::login::{Credentials, Device, Options, Session};
use sangfor_atrust::net::{HttpsPortal, dial};
use sangfor_atrust::tcp::{Destination, Identity, Stream};

/// The time the reference client gives a node to answer.
const NODE_TIMEOUT: Duration = Duration::from_secs(18);

#[expect(
    clippy::print_stderr,
    reason = "an example reports its progress to the terminal"
)]
fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let [
        mode,
        gateway_host,
        gateway_port,
        username,
        domain,
        host,
        port,
    ] = arguments.as_slice()
    else {
        eprintln!(
            "usage: tunnel --insecure <gateway host> <gateway port> <user> <login domain> <host> <port>"
        );
        process::exit(2);
    };
    if mode != "--insecure" {
        return Err(
            "this example requires --insecure to send credentials over unverified TLS".into(),
        );
    }
    let verification = tls::ServerVerification::Unverified;
    let password = env::var("ATRUST_PASSWORD").map_err(|_| "set ATRUST_PASSWORD")?;
    let port: u16 = port.parse()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
    let mut random = OsRandom;

    // A real client keeps its device identity; a gateway may bind an account to it.
    let gateway = Gateway::new(gateway_host.as_str(), gateway_port.parse()?);
    let mut session = Session::new(gateway.clone(), Device::generate(&mut random)?);
    let credentials = Credentials {
        username,
        password: &password,
        domain,
    };
    let mut portal = HttpsPortal::new(&gateway, verification, OsRandom);
    let seconds = i64::try_from(now.as_secs())?;
    let login = session.login(
        &mut portal,
        &credentials,
        Options::default(),
        &mut random,
        seconds,
    )?;
    eprintln!(
        "logged in as {:?}; anti-MITM checks: {:?}",
        login.user_name, login.anti_mitm
    );

    // This client has no L3 tunnel, so it takes TCP that the policy would give to one.
    let resolve = |name: &str| {
        eprintln!(
            "the policy leaves {name} to the client to resolve, which this example does not do"
        );
        None
    };
    let destination = Destination::route(&login.policy, host, port, true, resolve)
        .ok_or("the policy does not send that destination through the gateway")?;
    let node = login
        .policy
        .node(&destination.node_group_id)
        .ok_or("the policy names no node")?;
    eprintln!("{} goes through {node}", destination.dest_addr);

    let micros = u64::try_from(now.as_micros())?;
    let device_id = session.device().id.clone();
    let identity = Identity::new(
        login.sid.as_str(),
        login.user_name.as_str(),
        device_id,
        micros,
        &mut random,
    )?;
    let transport = dial(node, NODE_TIMEOUT, verification, &mut random)?;
    let mut stream = Stream::open(transport, &identity, &destination, login.zero_rtt)?;

    // The node had its time to answer; the stream itself may be as slow as the far host is.
    stream.get_ref().get_ref().set_read_timeout(None)?;

    // An HTTP/1.0 request, which the far host answers by closing: no half-close is needed.
    write!(stream, "GET / HTTP/1.0\r\nHost: {host}\r\n\r\n")?;
    stream.flush()?;
    let mut answer = Vec::new();
    // A node that ends the connection without a TLS goodbye makes this fail after the data.
    if let Err(error) = stream.read_to_end(&mut answer) {
        eprintln!("the stream ended with: {error}");
    }
    io::stdout().write_all(&answer)?;
    Ok(())
}
