#!/bin/sh
# Verifica lo stato di GitHub Advanced Security sul repo.
# Richiede la chiave GITHUB_TOKEN (tab Keys del progetto) e il repo già
# pubblicato su GitHub. Non stampa mai il valore del token.
set -e

TOKEN="${GITHUB_TOKEN:-}"
if [ -z "$TOKEN" ]; then
  echo "ERRORE: GITHUB_TOKEN mancante — aggiungilo nella tab Keys del progetto." >&2
  exit 1
fi

REPO="${GITHUB_REPO:-matrixNeo76/wasmbox}"
API="https://api.github.com/repos/$REPO"
TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT

echo "Repo: $REPO"
echo "---"

check() {
  label="$1"
  path="$2"
  code=$(curl -s -o "$TMP" -w "%{http_code}" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Accept: application/vnd.github+json" \
    -H "X-GitHub-Api-Version: 2022-11-28" \
    "$API/$path")
  total=$(grep -o '"total_count":[0-9]*' "$TMP" | head -1 | cut -d: -f2)
  case "$code" in
    200) echo "$label: HTTP 200 — alert aperti: ${total:-0}" ;;
    401) echo "$label: HTTP 401 — token non valido o revocato" ;;
    403) echo "$label: HTTP 403 — token senza permessi (scope 'repo'/'security_events')" ;;
    404) echo "$label: HTTP 404 — repo inesistente o funzionalità non abilitata (serve GitHub Advanced Security sul repo privato; gratis su repo pubblici)" ;;
    *)   echo "$label: HTTP $code" ;;
  esac
}

check "Code scanning  " "code-scanning/alerts?state=open&per_page=1"
check "Secret scanning" "secret-scanning/alerts?state=open&per_page=1"
check "Dependabot     " "dependabot/alerts?state=open&per_page=1"
