#!/bin/sh
# Scenario C — guest ostili (docs/integration.md, sezione errori/checklist).
#
# Compila le 3 bombe WAT (wat2wasm della dev-dep `wat`? qui via `wat` bin
# non esiste: usiamo la libreria `wat` già in dev-dep con un mini helper
# Rust; se preferisci un tool esterno, sostituisci compile_wat()).
set -u
cd "$(dirname "$0")/../.."   # project root

BIN=target/debug/wasmbox-cli
DIR=examples/scenario-hostile-guest
OUT=target/scenario-hostile
mkdir -p "$OUT"

if [ ! -x "$BIN" ]; then
  echo '{"scenario":"C","ok":false,"reason":"compila prima: cargo build -p wasmbox-cli"}'
  exit 2
fi

FAILS=""

case_run_fuel() {  # NAME ERR1 ERR2 — accetta exit 3 (fuel) O 4 (timeout):
  # per un loop infinito il runtime NON è deterministico su quale limite
  # scatta per primo (fuel 1e9 vs epoch 1s, dipende dal so carico della macchina):
  # entrambi sono risposte valide al limite.
  name=$1
  printf 'case %s: ' "$name"
  out=$("$BIN" run "$OUT/$name.wasm" "x" --json 2>/dev/null)
  code=$?
  if { [ "$code" -eq 3 ] || [ "$code" -eq 4 ]; } \
     && printf '%s' "$out" | grep -q '"ok":false' \
     && printf '%s' "$out" | grep -q 'fuel\|timeout'; then
    echo "PASS (exit=$code, limite computazione attivo)"
  else
    echo "FAIL (exit=$code atteso=3|4, json=${out:-<null>})"
    FAILS="$FAILS $name"
  fi
}

case_run() {  # NAME EXPECT_EXIT EXPECT_ERROR INPUT("")
  name=$1; exit_exp=$2; err_exp=$3; input=$4
  printf 'case %s: ' "$name"
  if [ -n "$input" ]; then
    out=$("$BIN" run "$OUT/$name.wasm" "$input" --json 2>/dev/null)
  else
    out=$("$BIN" run "$OUT/$name.wasm" --json <"$OUT/oom-input.bin" 2>/dev/null)
  fi
  code=$?
  if [ "$code" = "$exit_exp" ] && printf '%s' "$out" | grep -q '"ok":false' \
     && printf '%s' "$out" | grep -q "$err_exp"; then
    echo "PASS (exit=$code, error="$err_exp")"
  else
    echo "FAIL (exit=$code atteso=$exit_exp, json=${out:-<null>})"
    FAILS="$FAILS $name"
  fi
}

# --- fuel bomb: loop infinito → exit 3 (fuel) oppure 4 (timeout):
#     limite attivo in entrambi i casi (risposta valida, vedasi funzione sopra)
case_run_fuel fuel-bomb

# --- oom bomb: input 20 MiB via stdin → GuestOutOfMemory, exit 5 ---
head -c 20971520 /dev/zero | tr '\0' 'x' > "$OUT/oom-input.bin"
case_run oom-bomb 5 out_of_memory ""

# --- ask flood: 100k richieste → errore ask (CLI: ask_limit_or_payload), exit 6 ---
case_run ask-flood 6 ask_limit "ping"

echo
if [ -z "$FAILS" ]; then
  echo '{"scenario":"C","ok":true,"all":"limits-enforced-as-expected"}'
  exit 0
else
  echo "{\"scenario\":\"C\",\"ok\":false,\"failed\":\"$FAILS\"}"
  exit 1
fi
