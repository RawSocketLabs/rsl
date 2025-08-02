fn main() {
    let _build = cxx_build::bridge("src/lib.rs")
        .include("/Users/bluefoot/workspace/rsl/dsdcc/build/include/dsdcc/")
        .include("src")
        .file("src/rust_dsdcc.cpp")
        .flag_if_supported("-std=c++17")
        .compile("rust_dsdcc");

    println!("cargo:rerun-if-changed=src/rust_dsdcc.cc");
    println!("cargo:rerun-if-changed=src/rust_dsdcc.h");
    println!("cargo:rustc-link-lib=dsdcc");
    println!("cargo:rustc-link-search=native=/usr/local/lib/");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-W1,-rpath,@loader_path/../lib")
    }
}
