#!/bin/sh
# Scenario A — pipeline editoriale via CLI (docs/integration.md § 1).
#
# Simula un agente che riceve un guest .wasm e decide se fidarlo:
#   1. run normale (--json)                  → ok:true, exit 0
#   2. input ostile (3 casi)                 → ok:true oppure errore tipizzato, mai crash
#   3. decisione TRUST / REJECT              → registry/<sha256>.ok oppure exit 1
#   4. verdetto finale: UNA riga JSON macchina-parsabile
set -u
cd "$(dirname "$0")/../.."   # project root

BIN=target/debug/wasmbox-cli
GUEST="${1:-target/wasm32-unknown-unknown/release/guest_echo.wasm}"
REGISTRY=examples/scenario-cli-pipeline/registry
mkdir -p "$REGISTRY"

if [ ! -x "$BIN" ]; then
  echo '{"verdict":"reject","reason":"wasmbox-cli non compilato (cargo build -p wasmbox-cli)"}'
  exit 2
fi
if [ ! -f "$GUEST" ]; then
  echo '{"verdict":"reject","reason":"guest non trovato: compila guest-echo con sh scripts/build.sh"}'
  exit 2
fi

SHA=$(sha256sum "$GUEST" | cut -d' ' -f1)
FAILS=""

# run_case LABEL INPUT  — esegue una run --json e registra exit/errore
run_case() {
  label=$1
  input=$2
  out=$("$BIN" run "$GUEST" "$input" --json 2>/dev/null)
  code=$?
  printf 'case %s: exit=%d json=%s\n' "$label" "$code" "${out:-<null>}"
  case "$code" in
    0) ;;
    3|4|5|6|10) FAILS="$FAILS $label(resource: rivedere il guest, mai ritentare identico)";;
    8|9)        FAILS="$FAILS $label(shape: guest non conforme all'ABI)";;
    *)          FAILS="$FAILS $label(inesplicita: exit $code)";;
  esac
}

run_case input-normale "ciao dallo scenario A"
run_case input-vuoto ""
run_case input-grosso "$(head -c 102400 /dev/zero | tr '\0' 'x')"
# byte non-UTF8 (\xC3\x28 è una sequenza UTF-8 invalida)
run_case input-nonutf8 "$(printf '\xc3\x28')"

if [ -z "$FAILS" ]; then
  tmp="$REGISTRY/$SHA.ok.tmp"
  echo "$SHA $(date -u +%Y-%m-%dT%H:%M:%SZ)" >"$tmp" && mv "$tmp" "$REGISTRY/$SHA.ok"
  echo '{"verdict":"trust","reason":"all-cases-ok"}'
  echo "registrato fidato: registry/$SHA.ok"
  exit 0
else
  echo "{\"verdict\":\"reject\",\"reason\":\"failed-cases\",\"failed\":\"$FAILS\"}"
  echo "guest NON registrato." >&2
  exit 1
fi
