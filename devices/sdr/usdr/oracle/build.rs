//! Compiles the vendored libusdr subset plus the sim transport plugin into one static library.

use std::fs;
use std::path::Path;

fn main() {
    let libusdr = Path::new("libusdr");
    let lib = libusdr.join("lib");
    let sources =
        fs::read_to_string(libusdr.join("sources.txt")).expect("libusdr/sources.txt is vendored");

    println!("cargo:rerun-if-changed=libusdr");
    println!("cargo:rerun-if-changed=shim");

    let includes = ["", "port", "lowlevel", "common", "models", "xdsp"].map(|dir| lib.join(dir));

    // The shim is ours, so it builds with -Wall -Werror in its own archive. cc's default
    // warning set would add -Wextra, which trips on unused parameters in libusdr's headers.
    cc::Build::new()
        .file("shim/sim_plugin.c")
        .includes(&includes)
        .flag("-std=gnu11")
        .warnings(false)
        .flag("-Wall")
        .flag("-Werror")
        .compile("usdr_oracle_shim");

    // Compile exact upstream USB RX functions over a resource-free buffer boundary.
    // The include retains upstream's unused parameters; its warnings are not ours to fix.
    cc::Build::new()
        .file("shim/usb_rx.c")
        .includes(&includes)
        .flag("-std=gnu11")
        .warnings(false)
        .compile("usdr_oracle_usb_rx");

    cc::Build::new()
        .files(sources.lines().map(|source| lib.join(source)))
        .includes(&includes)
        .include(libusdr.join("gen"))
        .flag("-std=gnu11")
        // No fused multiply-adds, so float results compare bit for bit with Rust's scalar
        // ports on every target (GCC fuses by default in GNU mode where FMA exists).
        .flag("-ffp-contract=off")
        // Upstream's own warnings are not ours to fix.
        .warnings(false)
        .define("USDR_DEFAULT_AFECFG_PATH", "\"/nonexistent\"")
        .define("USDR_DEFAULT_AFECAPI", "\"/nonexistent.so\"")
        .compile("usdr_oracle");

    for system in ["m", "pthread", "dl"] {
        println!("cargo:rustc-link-lib={system}");
    }
}
