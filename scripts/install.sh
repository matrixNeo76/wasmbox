#!/bin/sh
# Install idempotente del toolchain Rust per wasmbox.
# Durable: rieseguibile in ogni ambiente pulito.
set -e

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo assente: installo rustup (stable + wasm32-unknown-unknown)"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    --default-toolchain stable \
    --profile minimal \
    -t wasm32-unknown-unknown
fi

# shellcheck disable=SC1091
. "$HOME/.cargo/env"

rustup target add wasm32-unknown-unknown
rustup component add clippy rustfmt

cargo --version
rustc --version
