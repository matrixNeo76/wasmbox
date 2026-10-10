#!/bin/sh
# Compila gli artefatti degli scenari:
#   - guest B: examples/scenario-tool-handler/guest → wasm32 (cdylib);
#   - bombe C: examples/scenario-hostile-guest/*.wat → .wasm (via helper Rust
#     `wat` — modular: il crate `wat` è già dev-dep del workspace).
set -eu
cd "$(dirname "$0")/../.."

. "$HOME/.cargo/env" 2>/dev/null || true

OUT=target/scenario-hostile
mkdir -p "$OUT"

# --- guest B ---
cargo build -p scenario-tool-guest --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/scenario_tool_guest.wasm "$OUT/" 2>/dev/null || true

# --- bombe C via helper Rust (cargo script trampolino) ---
# Usiamo un piccolo caso di test dell'helper: compiles all WATs via `wat`.
# (Non ricorriamo a binari esterni: il crate `wat` è già dev-dep autorizzata.)
cargo run -q -p wat-compile-scenarios -- examples/scenario-hostile-guest "$OUT"
echo "scenari compilati in $OUT:"
ls -1 "$OUT"
