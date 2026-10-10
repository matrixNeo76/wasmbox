#!/bin/sh
# Scenario F — tool LLM reale via OpenRouter (docs/extension-plan.md §4).
#
# Il tool `llm:<prompt>` vive SOLO nell'host handler (feature `llm` del
# crate scenario-tool-handler); la sandbox resta identica. PROVE:
#
#   1) offline: senza OPENROUTER_API_KEY la run fallisce con
#      `SandboxError::Host` → exit 7 e messaggio "llm:0: OPENROUTER_API_KEY"
#      ⇒ dimostra che l'errore di rete/credenziali resta tipizzato;
#   2) online (SOLO se la chiave è nell'env del processo): prompt reale →
#      `tool_result:llm=<testo>` nel diff con exit 0.
#
# Valori attesi sensibili: la chiave NON viene mai stampata né tracciata.
set -u
cd "$(dirname "$0")/../.."

. "$HOME/.cargo/env" 2>/dev/null || true

BIN=target/debug/scenario-tool-handler
GUEST=target/wasm32-unknown-unknown/release/scenario_tool_guest.wasm

if [ ! -x "$BIN" ]; then
  echo '{"scenario":"F","ok":false,"reason":"compila prima: cargo build -p scenario-tool-handler --features llm"}'
  exit 2
fi
[ -f "$GUEST" ] || {
  echo '{"scenario":"F","ok":false,"reason":"compila il guest: cargo build -p scenario-tool-guest --target wasm32-unknown-unknown --release"}'
  exit 2
}

FAILS=""
printf 'case llm-no-key-offline: '
# Nega la chiave anche se presente nel shell env: deve fallire con exit 7
# e messaggio esplicito (che non rivela mai la chiave).
unset OPENROUTER_API_KEY || true
out=$(env -u OPENROUTER_API_KEY "$BIN" "$GUEST" "llm:ping" 2>&1)
code=$?
if [ "$code" -eq 7 ] && printf '%s' "$out" | grep -q 'llm:0: OPENROUTER_API_KEY'; then
  echo "PASS (exit=7, SandboxError::Host tipizzato)"
else
  echo "FAIL (exit=$code, out=${out:-<null>})"
  FAILS="$FAILS llm-no-key-offline"
fi

if [ -n "${OPENROUTER_API_KEY:-}" ]; then
  printf 'case llm-real-call: '
  out=$("$BIN" "$GUEST" "llm:Rispondi con una sola parola: ok" 2>&1)
  code=$?
  if [ "$code" -eq 0 ] && printf '%s' "$out" | grep -q 'tool_result:llm='; then
    echo "PASS (exit=0, risposta reale ricevuta)"
  else
    echo "FAIL (exit=$code, out=${out:-<null>})"
    FAILS="$FAILS llm-real-call"
  fi
else
  printf 'case llm-real-call: SKIP (OPENROUTER_API_KEY non presente: nessuna chiamata di rete)\n'
fi

echo "---"
if [ -z "$FAILS" ]; then
  echo "ALL F CASES PASS (o SKIP)]"
  exit 0
fi
echo "FAILED:$FAILS"
exit 1
