//! Smoke test: the facade namespaces resolve for the pure-Rust feature slice CI builds.
//! Not exhaustive — it proves the re-export paths exist and unify, not the crates' behavior.

#[test]
fn owned_and_external_paths_resolve() {
    // owned: codec is bnb
    #[cfg(feature = "codec")]
    fn _codec_is_bnb() {
        let _ = core::any::type_name::<rsl::codec::Error>();
    }

    // owned: protocol crates live under rsl::proto::*
    #[cfg(feature = "proto-dns")]
    fn _proto_dns() {
        let _ = core::any::type_name::<rsl::proto::dns::Message>();
    }

    // external: blessed crates live under rsl::ext::*
    #[cfg(feature = "serde")]
    fn _ext_serde() {
        let _ = core::any::type_name::<rsl::ext::serde_json::Value>();
    }

    // prelude glob compiles
    #[allow(unused_imports)]
    use rsl::prelude::*;
}
