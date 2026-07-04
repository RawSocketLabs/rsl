//! Smoke test: the owned-library namespaces resolve for the feature slice CI builds.
//! Not exhaustive — it proves the re-export paths exist and unify, not the crates' behavior.

#[test]
fn owned_paths_resolve() {
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

    // prelude glob compiles
    #[allow(unused_imports)]
    use rsl::prelude::*;
}
