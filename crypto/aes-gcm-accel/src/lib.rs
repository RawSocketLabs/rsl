//! Checked AES-NI and PCLMULQDQ primitives. Key expansion and the GCM construction
//! remain in the portable crypto crate.
#![no_std]
#![cfg(feature = "x86")]

#[cfg(all(
    target_arch = "x86_64",
    not(target_env = "sgx"),
    not(target_os = "none")
))]
mod x86;
#[cfg(all(
    target_arch = "x86_64",
    not(target_env = "sgx"),
    not(target_os = "none")
))]
pub use x86::AesGcmCpu;

#[cfg(not(all(
    target_arch = "x86_64",
    not(target_env = "sgx"),
    not(target_os = "none")
)))]
mod unsupported;
#[cfg(not(all(
    target_arch = "x86_64",
    not(target_env = "sgx"),
    not(target_os = "none")
)))]
pub use unsupported::AesGcmCpu;
