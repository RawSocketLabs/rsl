//! **resolve** — a tiny `dig`: query a public DNS resolver for a name's A/AAAA records.
//!
//! Run with: `cargo run -p dns --features client --example resolve -- example.com`
//! (needs network; queries `1.1.1.1:53` over UDP).
#![allow(clippy::print_stdout, clippy::print_stderr)]

use dns::Resolver;

fn main() {
    let name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "example.com".to_string());
    let resolver = Resolver::new("1.1.1.1:53".parse().unwrap());

    println!("resolving {name} via 1.1.1.1:53");
    match resolver.resolve_ipv4(&name) {
        Ok(ips) => {
            println!("A:");
            for ip in ips {
                println!("  {ip}");
            }
        }
        Err(e) => eprintln!("A query failed: {e}"),
    }
    match resolver.resolve_ipv6(&name) {
        Ok(ips) => {
            println!("AAAA:");
            for ip in ips {
                println!("  {ip}");
            }
        }
        Err(e) => eprintln!("AAAA query failed: {e}"),
    }
}
