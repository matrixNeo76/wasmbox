#!/bin/sh
# Preview: esegue la suite di test di wasmbox e serve il report su 0.0.0.0:$PORT.
set -e
cd "$(dirname "$0")/.."

# shellcheck disable=SC1091
. "$HOME/.cargo/env" 2>/dev/null || true

PORT="${PORT:-8080}"
REPORT=target/preview-report.txt
SITE=target/preview-site
mkdir -p "$SITE"

STATUS=PASS
DURATION="n/a"

if command -v cargo >/dev/null 2>&1; then
  # Il guest wasm32 (se presente) abilita il test e2e facoltativo.
  if [ -f target/wasm32-unknown-unknown/release/guest_echo.wasm ]; then
    export GUEST_ECHO_WASM="$PWD/target/wasm32-unknown-unknown/release/guest_echo.wasm"
  fi

  START=$(date +%s)
  if cargo test -p wasmbox-core >"$REPORT" 2>&1; then
    STATUS=PASS
  else
    STATUS=FAIL
  fi
  DURATION="$(( $(date +%s) - START ))s"

  # Demo CLI (output JSON di una run reale) + screenshot UI headless.
  if [ -f target/debug/wasmbox-cli ] && \
     [ -f target/wasm32-unknown-unknown/release/guest_echo.wasm ]; then
    CLI_DEMO=$(target/debug/wasmbox-cli run \
      target/wasm32-unknown-unknown/release/guest_echo.wasm "ciao dal preview" \
      --json 2>/dev/null)
    export CLI_DEMO
  fi
  if [ -f target/debug/wasmbox-ui ] && \
     [ -f target/wasm32-unknown-unknown/release/guest_echo.wasm ]; then
    if target/debug/wasmbox-ui --screenshot target/preview-ui.bmp \
      target/wasm32-unknown-unknown/release/guest_echo.wasm "demo" \
      >/dev/null 2>&1; then
      UI_SCREENSHOT_BMP="$PWD/target/preview-ui.bmp"
      export UI_SCREENSHOT_BMP
    fi
  fi

else
  echo "cargo non disponibile nel preview" >"$REPORT"
  STATUS=FAIL
fi

export STATUS REPORT DURATION
python3 scripts/render_report.py >"$SITE/index.html"

echo "report pronto: status=$STATUS durata=$DURATION porta=$PORT"
exec python3 -m http.server "$PORT" --bind 0.0.0.0 --directory "$SITE"
