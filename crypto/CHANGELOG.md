# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/RawSocketLabs/rsl/releases/tag/rsl-crypto-v0.1.0) - 2026-08-29

### Added

- *(pki)* add x509 validation stack
- *(crypto)* stream authenticated record plaintext
- *(crypto)* stream AEAD records into fallible sinks
- *(crypto)* add staged AEAD record streaming
- *(crypto)* add Ed448 and Ed448ph
- *(crypto)* add X448
- *(crypto)* add SHA3-256 and SHAKE256
- *(crypto)* add P-384 ECDH and ECDSA over a generic Weierstrass core
- *(crypto)* add AES-256 and AES-256-GCM
- *(crypto)* add SHA-384, HMAC-SHA-384, and HKDF-SHA-384
- *(crypto)* add ChaCha20, Poly1305, and ChaCha20-Poly1305
- *(crypto)* add Ed25519ctx and Ed25519ph variants
- *(crypto)* add RSA primitive and RSASSA-PSS verification
- *(crypto)* add deterministic ECDSA P-256 signing
- *(crypto)* add P-256 ECDH and ECDSA verification
- *(crypto)* add readable modern and legacy primitives

### Other

- *(pki)* harden certificate validation boundaries
- *(crypto)* add algorithm-selection guide
- *(crypto)* add fuzz targets, side-channel review record, and million-iteration checkpoints
- *(crypto)* backtick identifiers in the Ed448 teaching page
