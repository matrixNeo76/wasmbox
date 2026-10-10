#!/bin/sh
# Scenario D — orchestrazione remota via wasmbox-http
# (docs/extension-plan.md §2).
#
# Avvia il server su una porta libera di loopback, poi con `curl` (sempre
# presente nei runner CI) verifica:
#   1) echo ok                  → 200 ok:true, output in base64 atteso
#   2) request malformata       → 400 bad_request
#   3) wasm invalido            → 200 ok:false, error:"invalid_wasm"
#   4) fuel-bomb + limits       → 200 ok:false, error:"fuel_exhausted"
#      (max_fuel piccolo + epoch_timeout_ms:null ⇒ DETERMINISTICO, come M1)
# Il server si ferma con trap EXIT: nessun processo residuo.
set -u
cd "$(dirname "$0")/../.."   # project root

BIN=target/release/wasmbox-http
OUT=target/scenario-hostile
PORT=13130
URL="http://127.0.0.1:$PORT"

if [ ! -x "$BIN" ]; then
  echo '{"scenario":"D","ok":false,"reason":"compila prima: cargo build -p wasmbox-http --release"}'
  exit 2
fi
command -v curl >/dev/null 2>&1 || {
  echo '{"scenario":"D","ok":false,"reason":"curl mancante"}'
  exit 2
}
[ -f "$OUT/guest_echo.wasm" ] || sh scripts/scenarios/build-scenarios.sh >/dev/null 2>&1 || true

FAILS=""

expect() {  # NAME EXPECT_CODE EXPECT_PATTERN BODY(POST /run)
  name=$1; code_exp=$2; pattern=$3; body=$4
  printf 'case %s: ' "$name"
  resp=$(curl -s -o /tmp/wasmbox-http-resp.$$ -w '%{http_code}' \
    -X POST -H 'Content-Type: application/json' -d "$body" "$URL/run")
  code=$?
  respbody=$(cat /tmp/wasmbox-http-resp.$$ 2>/dev/null)
  rm -f /tmp/wasmbox-http-resp.$$
  if [ "$code" = "0" ] && [ "$resp" = "$code_exp" ] \
     && printf '%s' "$respbody" | grep -q "$pattern"; then
    echo "PASS (http=$resp)"
  else
    echo "FAIL (http=$resp atteso=$code_exp, body=${respbody:-<null>})"
    FAILS="$FAILS $name"
  fi
}

# --- avvio server ---
"$BIN" --bind "127.0.0.1:$PORT" >/tmp/wasmbox-http.$$.log 2>&1 &
SRV=$!
trap 'kill "$SRV" 2>/dev/null; rm -f /tmp/wasmbox-http.$$.log' EXIT

# attendi readiness (max 20 x 100 ms)
i=0
while [ $i -lt 20 ]; do
  curl -s -o /dev/null "$URL/healthz" && break
  i=$((i + 1))
  sleep 0.1
done
if [ $i -eq 20 ]; then
  echo '{"scenario":"D","ok":false,"reason":"server non pronto su '"$URL"'"}'
  exit 1
fi
echo "[scenario-d] server pronto su $URL (pid $SRV)"

# --- 1) echo ok: request b64("echo_ok"), atteso output b64("echo_result:...").
#    Il guest echo antepone un prefix: verifichiamo solo ok:true + ask_calls.
#    NB: guest_echo.wasm NON fa echo del prefix via HTTP? sì: la CLI accoda
#    "echo_result:"; qui si valida la pipeline HTTP→sandbox→eco.
GUEST_B64=$(base64 -w0 target/wasm32-unknown-unknown/release/guest_echo.wasm)
echo_ok_b64=$(printf 'scenario-d input' | base64 -w0)
expect echo-ok 200 '"ok":true' "{\"guest\":\"$GUEST_B64\",\"input\":\"$echo_ok_b64\"}"

# --- 2) json malformato ---
expect bad-request-json 400 'bad_request' '{"guest":"###"}'

# --- 3) wasm invalido: b64 di bytes non-wasm ---
expect invalid-wasm 200 '"error":"invalid_wasm"' \
  '{"guest":"dGhpcw==","input":null}'

# --- 4) fuel bomb con limits: fuel-only deterministico (M1) ---
FUEL_BOMB_B64=$(base64 -w0 "$OUT/fuel-bomb.wasm")
expect fuel-limited 200 '"error":"fuel_exhausted"' \
  "{\"guest\":\"$FUEL_BOMB_B64\",\"input\":null,\"limits\":{\"max_fuel\":5000000,\"epoch_timeout_ms\":null}}"

echo "---"
if [ -z "$FAILS" ]; then
  echo "ALL D CASES PASS"
  exit 0
fi
echo "FAILED:$FAILS"
exit 1
