# CircleCI job image: every toolchain, cargo tool and native library the CI profiles use,
# pinned here so a run is reproducible from the image digest. Published as
# ghcr.io/rawsocketlabs/rsl-ci by the CircleCI `image` job; scripts/ci/circleci.py and
# .circleci/config.yml reference it by digest, and test_circleci.py holds the pins in step.
FROM docker.io/library/ubuntu:24.04@sha256:534baea6a22c03a63003dbc8dbe78fe34bc0d7e595d9a9dc9834884ff530eb55

ARG STABLE=1.99.0
ARG MSRV=1.85.0
# One pinned nightly serves both the public-API gate (.github/ci/bnb.toml) and fuzzing.
ARG NIGHTLY=nightly-2026-06-17
ARG NEXTEST=0.9.143
ARG NEXTEST_SHA256=66786b9abe23920d022a182d1416b1bbc8130dd4872a9553d76985a1708dcd1e
ARG DENY=0.20.2
ARG DENY_SHA256=9f12ed4c49936e09b48bf862b595cde2fe64fcbd9d74dfacac6131ca824c8d5f
ARG RELEASE_PLZ=0.3.172
ARG RELEASE_PLZ_SHA256=84b7013d164e1fc780b31dc9ef190977830599d6cb5ce81c993f6fcbd3cdc1aa
ARG FUZZ=0.13.2
ARG PUBLIC_API=0.52.0
ARG SEMVER=0.50.0
ARG DSDCC=f27b32d2df131ae3a376fe72d3fb880ae1f9ede1

LABEL org.opencontainers.image.source=https://github.com/RawSocketLabs/rsl
LABEL org.opencontainers.image.description="RawSocket Labs rsl CI job image"

SHELL ["/bin/bash", "-euo", "pipefail", "-c"]

# CircleCI's checkout and caches need git, ssh, tar, gzip and CA roots; the FFI profiles need a
# C/C++ compiler, libclang and libbladeRF; cargo-public-api builds against OpenSSL.
RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      build-essential ca-certificates curl git gzip libbladerf-dev libclang-dev libssl-dev openssh-client \
      pkg-config python3 tar xz-utils \
 && rm -rf /var/lib/apt/lists/*

# DSDcc headers for the rust-dsdcc compile check (no native linking).
RUN git init -q /opt/dsdcc \
 && git -C /opt/dsdcc fetch -q --depth 1 https://github.com/f4exb/dsdcc.git "${DSDCC}" \
 && git -C /opt/dsdcc checkout -q FETCH_HEAD \
 && rm -rf /opt/dsdcc/.git

# Jobs run unprivileged, like the Actions runner user: raw-socket tests see no root.
RUN useradd --create-home --uid 1001 ci
USER ci
WORKDIR /home/ci
# RUSTUP_TOOLCHAIN overrides rust-toolchain.toml's floating `stable`, so a job never installs
# a toolchain at run time; jobs that need another toolchain set it themselves.
ENV PATH=/home/ci/.cargo/bin:${PATH} \
    RUSTUP_TOOLCHAIN=${STABLE} \
    DSDCC_INCLUDE=/opt/dsdcc \
    CARGO_TERM_COLOR=always

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --profile minimal --default-toolchain none --no-modify-path \
 && for toolchain in "${STABLE}" "${MSRV}" "${NIGHTLY}"; do \
      rustup toolchain install "${toolchain}" --profile minimal --component rustfmt,clippy,rust-src; \
    done \
 && rustup target add thumbv7em-none-eabi --toolchain "${STABLE}" \
 && rustup default "${STABLE}"

# Released binaries, verified against their published SHA-256.
RUN fetch_tool() { \
      curl --proto '=https' --tlsv1.2 -sSfL -o /tmp/tool.tar.gz "$1" \
      && echo "$2  /tmp/tool.tar.gz" | sha256sum --check --quiet \
      && tar -xzf /tmp/tool.tar.gz -O "$3" >"${HOME}/.cargo/bin/$4" \
      && chmod +x "${HOME}/.cargo/bin/$4" && rm /tmp/tool.tar.gz; }; \
    fetch_tool "https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-${NEXTEST}/cargo-nextest-${NEXTEST}-x86_64-unknown-linux-gnu.tar.gz" \
      "${NEXTEST_SHA256}" cargo-nextest cargo-nextest \
 && fetch_tool "https://github.com/EmbarkStudios/cargo-deny/releases/download/${DENY}/cargo-deny-${DENY}-x86_64-unknown-linux-musl.tar.gz" \
      "${DENY_SHA256}" "cargo-deny-${DENY}-x86_64-unknown-linux-musl/cargo-deny" cargo-deny \
 && fetch_tool "https://github.com/release-plz/release-plz/releases/download/release-plz-v${RELEASE_PLZ}/release-plz-x86_64-unknown-linux-gnu.tar.gz" \
      "${RELEASE_PLZ_SHA256}" release-plz release-plz

# Built from crates.io at locked versions; the registry is dropped to keep the image small.
RUN cargo install --locked "cargo-fuzz@${FUZZ}" "cargo-public-api@${PUBLIC_API}" "cargo-semver-checks@${SEMVER}" \
 && rm -rf "${HOME}/.cargo/registry" "${HOME}/.cargo/git"
