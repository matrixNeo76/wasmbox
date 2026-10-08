#!/bin/sh
# Build del workspace + artefatto guest wasm32.
set -e
cd "$(dirname "$0")/.."

# shellcheck disable=SC1091
. "$HOME/.cargo/env"

cargo check --workspace
cargo build -p guest-echo --target wasm32-unknown-unknown --release

echo "artefatto guest: target/wasm32-unknown-unknown/release/guest_echo.wasm"
