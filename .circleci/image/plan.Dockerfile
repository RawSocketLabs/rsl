# CircleCI plan job image: only what scripts/ci/circleci.py plan runs (git, python3 and a bare
# cargo for `cargo metadata --offline --no-deps`), so a fresh cloud machine pulls ~100 MB instead
# of the full rsl-ci image before every pipeline can start. Published as
# ghcr.io/rawsocketlabs/rsl-ci-plan by the CircleCI `image` job beside rsl-ci, for the plan job in
# .circleci/config.yml. The gate recomputes the plan on rsl-ci, so a divergence fails loudly.
FROM docker.io/library/ubuntu:26.04@sha256:6a06111c3b0a8600feadb270aec771dc239bf407b25625f93f44ab00e11f4d3c AS cargo

# The full image's stable cargo (test_circleci.py holds it to the Dockerfile's STABLE).
ARG CARGO=1.100.0
ARG CARGO_SHA256=7e29ce07d7c6e92d1826870575ba27d57fff6cb3e754aa57d99099cb88fab35d

SHELL ["/bin/bash", "-euo", "pipefail", "-c"]
RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends ca-certificates curl xz-utils \
 && curl --proto '=https' --tlsv1.2 -sSfL -o /tmp/cargo.tar.xz \
      "https://static.rust-lang.org/dist/cargo-${CARGO}-x86_64-unknown-linux-gnu.tar.xz" \
 && echo "${CARGO_SHA256}  /tmp/cargo.tar.xz" | sha256sum --check --quiet \
 && tar -xJf /tmp/cargo.tar.xz -O "cargo-${CARGO}-x86_64-unknown-linux-gnu/cargo/bin/cargo" >/usr/local/bin/cargo \
 && chmod +x /usr/local/bin/cargo

FROM docker.io/library/ubuntu:26.04@sha256:6a06111c3b0a8600feadb270aec771dc239bf407b25625f93f44ab00e11f4d3c

LABEL org.opencontainers.image.source=https://github.com/RawSocketLabs/rsl
LABEL org.opencontainers.image.description="RawSocket Labs rsl CI plan job image"

SHELL ["/bin/bash", "-euo", "pipefail", "-c"]

# CircleCI's checkout needs git, ssh and CA roots; the plan calls GitHub and CircleCI over HTTPS.
RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      ca-certificates git openssh-client python3 \
 && rm -rf /var/lib/apt/lists/*
COPY --from=cargo /usr/local/bin/cargo /usr/local/bin/cargo

RUN useradd --create-home --uid 1001 ci
USER ci
WORKDIR /home/ci
