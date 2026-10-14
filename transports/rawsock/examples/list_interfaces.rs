//! **`list_interfaces`** — what can this host+process actually do, and on which interface?
//!
//! Prints the live [`capabilities`] probe (which layers open *right now*) and enumerates the
//! system's interfaces with their operational state, read from `/sys/class/net`. The other
//! socket examples take an interface name; this one shows you the candidates. Unprivileged.
//!
//! Run with: `cargo run -p rawsock --example list_interfaces`
#![allow(clippy::print_stdout)]

use rawsock::capabilities;

fn main() {
    let caps = capabilities();
    println!("openable layers (this host + process, right now):");
    println!("  transport (L4, UDP)      {}", mark(caps.transport));
    println!("  network   (L3, raw IPv4) {}", mark(caps.network));
    println!("  link      (L2, Ethernet) {}", mark(caps.link));
    if !caps.network && !caps.link {
        println!("  (L3/L2 need CAP_NET_RAW — rerun privileged to open them)");
    }

    println!("\ninterfaces (/sys/class/net):");
    match interfaces() {
        Ok(ifaces) if !ifaces.is_empty() => {
            for (name, state) in ifaces {
                println!("  {name:<12} {state}");
            }
        }
        Ok(_) => println!("  (none found)"),
        Err(e) => println!("  (could not read /sys/class/net: {e})"),
    }
}

fn mark(ok: bool) -> &'static str {
    if ok {
        "\u{2713} open"
    } else {
        "\u{2717} denied"
    }
}

/// Each interface name paired with its `operstate` (`up`, `down`, `unknown`, …), sorted by
/// name. `unknown` is normal for loopback and some virtual devices.
fn interfaces() -> std::io::Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir("/sys/class/net")? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        let state = std::fs::read_to_string(format!("/sys/class/net/{name}/operstate"))
            .map_or_else(|_| "?".to_owned(), |s| s.trim().to_owned());
        out.push((name, state));
    }
    out.sort();
    Ok(out)
}
