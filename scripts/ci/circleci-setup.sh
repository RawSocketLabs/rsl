#!/usr/bin/env bash
# Provision a CircleCI machine job: the Actions setup steps `scripts/ci/circleci.py` emits.
#
# Usage:
#   circleci-setup.sh rust TOOLCHAIN [COMPONENTS] [TARGETS]   # comma-separated lists
#   circleci-setup.sh tool CRATE VERSION                      # cargo install into ~/ci-tools/CRATE-VERSION
#   circleci-setup.sh release NAME VERSION URL SHA256 MEMBER  # verified released binary into ~/ci-tools
#   circleci-setup.sh apt PACKAGE...
set -euo pipefail

ensure_rustup() {
  if ! command -v rustup >/dev/null && [[ ! -x "${HOME}/.cargo/bin/rustup" ]]; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --profile minimal --default-toolchain none --no-modify-path
  fi
  # Later steps start a fresh shell that sources BASH_ENV.
  if [[ -n "${BASH_ENV:-}" ]] && ! grep -qs 'cargo/env' "${BASH_ENV}"; then
    # shellcheck disable=SC2016 # expanded when BASH_ENV is sourced
    echo 'export PATH="${HOME}/.cargo/bin:${PATH}"  # cargo/env' >>"${BASH_ENV}"
  fi
  export PATH="${HOME}/.cargo/bin:${PATH}"
}

on_path() {
  [[ -n "${BASH_ENV:-}" ]] && echo "export PATH=\"$1:\${PATH}\"" >>"${BASH_ENV}"
  export PATH="$1:${PATH}"
}

case "${1:-}" in
  rust)
    toolchain="$2" components="${3:-}" targets="${4:-}"
    ensure_rustup
    args=(toolchain install "${toolchain}" --profile minimal --no-self-update)
    [[ -n "${components}" ]] && args+=(--component "${components}")
    [[ -n "${targets}" ]] && args+=(--target "${targets}")
    rustup "${args[@]}"
    ;;
  tool)
    crate="$2" version="$3" root="${HOME}/ci-tools/$2-$3"
    ensure_rustup
    # A restored cache holds the binary; its own root keeps it independent of ~/.cargo/bin.
    if [[ ! -x "${root}/bin/${crate}" ]]; then
      rustup toolchain install stable --profile minimal --no-self-update
      cargo +stable install --locked --root "${root}" "${crate}@${version}"
    fi
    on_path "${root}/bin"
    ;;
  release)
    name="$2" version="$3" url="$4" digest="$5" member="$6" root="${HOME}/ci-tools/$2-$3"
    if [[ ! -x "${root}/bin/${name}" ]]; then
      archive="$(mktemp)"
      curl --proto '=https' --tlsv1.2 -sSfL -o "${archive}" "${url}"
      echo "${digest}  ${archive}" | sha256sum --check --quiet
      mkdir -p "${root}/bin"
      tar -xzf "${archive}" -O "${member}" >"${root}/bin/${name}"
      chmod +x "${root}/bin/${name}"
      rm -f "${archive}"
    fi
    on_path "${root}/bin"
    ;;
  apt)
    shift
    sudo apt-get update
    sudo apt-get install -y "$@"
    ;;
  *)
    echo "usage: $0 rust|tool|release|apt ..." >&2
    exit 2
    ;;
esac
