use std::env;

fn main() {
    let dsdcc_include =
        env::var("DSDCC_INCLUDE").expect("DSDCC_INCLUDE environment variable not set but required!");

    let _build = cxx_build::bridge("src/lib.rs")
        .include(dsdcc_include)
        .include("src")
        .file("src/rust_dsdcc.cpp")
        .flag_if_supported("-std=c++17")
        .compile("rust_dsdcc");

    println!("cargo:rerun-if-changed=src/rust_dsdcc.cpp");
    println!("cargo:rerun-if-changed=src/rust_dsdcc.h");
    println!("cargo:rustc-link-lib=dsdcc");
}
