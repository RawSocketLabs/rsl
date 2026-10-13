//! Compiles the vendored libusdr subset plus the sim transport plugin into one static library.

use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let libusdr = Path::new("libusdr");
    let lib = libusdr.join("lib");
    let sources =
        fs::read_to_string(libusdr.join("sources.txt")).expect("libusdr/sources.txt is vendored");

    println!("cargo:rerun-if-changed=libusdr");
    println!("cargo:rerun-if-changed=shim");

    let includes = ["", "port", "lowlevel", "common", "models", "xdsp"].map(|dir| lib.join(dir));

    // libusdr marks hot maths `optimize("-Ofast")`, a GCC attribute other compilers ignore,
    // and rsl-usdr's ports (the LMK05318 solver) reproduce what GCC makes of it. Another
    // compiler would build a different reference.
    let compiler = cc::Build::new().get_compiler();
    assert!(
        compiler.is_like_gnu() && !compiler.is_like_clang(),
        "the oracle needs GCC as its C compiler (CC), not {}",
        compiler.path().display()
    );

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

    // libusdr's LMK05318 planning on caller-supplied state, registers captured; ours too.
    cc::Build::new()
        .file("shim/lmk05318.c")
        .includes(&includes)
        .flag("-std=gnu11")
        .warnings(false)
        .flag("-Wall")
        .flag("-Werror")
        .compile("usdr_oracle_lmk05318");

    // Compile exact upstream USB RX functions over a resource-free buffer boundary.
    // The include retains upstream's unused parameters; its warnings are not ours to fix.
    cc::Build::new()
        .file("shim/usb_rx.c")
        .includes(&includes)
        .flag("-std=gnu11")
        .warnings(false)
        .compile("usdr_oracle_usb_rx");

    // Likewise the USB TX functions; their status read reaches a fake register file
    // through libusdr's own `lowlevel_get_ops`, so this archive precedes the main one.
    cc::Build::new()
        .file("shim/usb_tx.c")
        .includes(&includes)
        .flag("-std=gnu11")
        .warnings(false)
        .compile("usdr_oracle_usb_tx");

    // Header-only RTSA depth helper, exported for parity under the generic capability set.
    cc::Build::new()
        .file("shim/rtsa_depth.c")
        .includes(&includes)
        .flag("-std=gnu11")
        .flag("-ffp-contract=off")
        .warnings(false)
        .compile("usdr_oracle_rtsa_depth");

    // libusdr's flash update tool, its `main` renamed so the shim can run it like a
    // function (`rsl_oracle_usdr_flash`). It includes its headers relative to itself.
    cc::Build::new()
        .file(libusdr.join("tools/usdr_flash.c"))
        .includes(&includes)
        .include(libusdr.join("gen"))
        .flag("-std=gnu11")
        .define("main", "rsl_oracle_usdr_flash_main")
        .warnings(false)
        .compile("usdr_oracle_usdr_flash");

    // usdr_dm_create's TX sources and usdr_dm_gpsdo's discipline loop, each tool compiled
    // whole from libusdr/tools with its `main` renamed.
    compile_wrapping(
        Path::new("shim/dm_create.c"),
        "usdr_oracle_dm_create",
        &includes,
    );
    compile_wrapping(Path::new("shim/gpsdo.c"), "usdr_oracle_gpsdo", &includes);

    // nco.c has no generic path: without SSSE3 it compiles a stub that writes nothing. On
    // x86 it is built alone with SSSE3 so the NCO port can be compared; elsewhere it stays
    // in the main build as libusdr ships it.
    let x86 = matches!(
        std::env::var("CARGO_CFG_TARGET_ARCH").as_deref(),
        Ok("x86" | "x86_64")
    );
    let nco = "xdsp/nco.c";
    let sincos = "xdsp/sincos_functions.c";
    if x86 {
        cc::Build::new()
            .file(lib.join(nco))
            .includes(&includes)
            .include(libusdr.join("gen"))
            .flag("-std=gnu11")
            .flag("-ffp-contract=off")
            .flag("-mssse3")
            .warnings(false)
            .compile("usdr_oracle_nco");
    }

    cc::Build::new()
        .files(
            sources
                .lines()
                .filter(|source| !(x86 && *source == nco) && *source != sincos)
                .map(|source| lib.join(source)),
        )
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

    // The tone and chirp kernels those generators call, built alone to wrap likewise; their
    // arithmetic is otherwise unchanged. Linked after the main archive, whose NCO calls
    // them on other targets.
    compile_wrapping(&lib.join(sincos), "usdr_oracle_sincos", &includes);

    for system in ["m", "pthread", "dl"] {
        println!("cargo:rustc-link-lib={system}");
    }
}

/// Compiles `file` alone into `archive` with `-fwrapv`, which defines the signed overflow
/// the tools' carried phases and chirp steps reach within a few samples (GCC's unoptimised
/// code wraps there anyway), so they compare with rsl-usdr's wrapping sums. No fused
/// multiply-adds, as in the main build.
fn compile_wrapping(file: &Path, archive: &str, includes: &[PathBuf]) {
    cc::Build::new()
        .file(file)
        .includes(includes)
        .include(Path::new("libusdr").join("gen"))
        .flag("-std=gnu11")
        .flag("-ffp-contract=off")
        .flag("-fwrapv")
        .warnings(false)
        .compile(archive);
}
