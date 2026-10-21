//! Compiles the vendored libusdr subset plus the sim transport plugin into static libraries.
//!
//! The vendored tree is `devices/vendor/libusdr` (`../../../vendor/libusdr`), the one the
//! uSDR oracle compiles: one tree, one PIN; it already includes `device/pe_sync` and its
//! clock chips. It is read, never copied or edited.
//!
//! The chip drivers whose lock waits read the wall clock (`clock_get_time`, which calls
//! `clock_gettime`) are built in their own archive with that call renamed to the shim's
//! virtual clock, so their timeouts follow the board's virtual time, as their `usleep`s do.

use std::fs;
use std::path::Path;

/// The sources whose lock waits read the wall clock: every user of `cal/opt_func.h`'s
/// `clock_get_time` in the vendored tree.
const WALL_CLOCK_WAITS: [&str; 3] = [
    "hw/lmk05318/lmk05318.c",
    "hw/lmx1204/lmx1204.c",
    "hw/lmx2820/lmx2820.c",
];

fn main() {
    let libusdr = Path::new("../../../vendor/libusdr");
    let lib = libusdr.join("lib");
    let sources =
        fs::read_to_string(libusdr.join("sources.txt")).expect("libusdr/sources.txt is vendored");

    println!("cargo:rerun-if-changed={}", libusdr.display());
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
        .compile("pe_sync_oracle_shim");

    let (timed, untimed): (Vec<&str>, Vec<&str>) = sources
        .lines()
        .partition(|source| WALL_CLOCK_WAITS.contains(source));
    assert_eq!(
        timed.len(),
        WALL_CLOCK_WAITS.len(),
        "every wall-clock lock wait source is vendored"
    );
    let libusdr_build = |sources: &[&str]| {
        let mut build = cc::Build::new();
        build
            .files(sources.iter().map(|source| lib.join(source)))
            .includes(&includes)
            .include(libusdr.join("gen"))
            .flag("-std=gnu11")
            // Upstream's own warnings are not ours to fix.
            .warnings(false)
            .define("USDR_DEFAULT_AFECFG_PATH", "\"/nonexistent\"")
            .define("USDR_DEFAULT_AFECAPI", "\"/nonexistent.so\"");
        build
    };
    libusdr_build(&untimed).compile("pe_sync_oracle");
    libusdr_build(&timed)
        .define("clock_gettime", "pe_sync_oracle_clock_gettime")
        .compile("pe_sync_oracle_timed");

    for system in ["m", "pthread", "dl"] {
        println!("cargo:rustc-link-lib={system}");
    }
}
