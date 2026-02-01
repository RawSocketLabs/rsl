
fn main() {
    let _build = cxx_build::bridge("src/lib.rs")
        .file("src/usdr_wrapper.cpp")
        .include("include")
        .flag_if_supported("-std=c++17")
        .compile("usdr_cxx");

    println!("cargo:rustc-link-lib=usdr");
}
