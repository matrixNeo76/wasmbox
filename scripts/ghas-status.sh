#!/bin/sh
# Verifica lo stato di GitHub Advanced Security sul repo.
#
# NOTA (2026-10-09, verificata con prove): la credenziale gestita del
# terminale Freebuff viene iniettata per i comandi `gh` diretti, MA NON
# si propaga ai sottoprocessi di questo script → gh risponde "gh auth login".
# Lo script quindi DEVE ricevere un token esplicito nel processo:
#   GH_TOKEN=… sh ./scripts/ghas-status.sh     (token utente, scope repo)
# GITHUB_REPO permette di interrogare un altro repo.
set -e

REPO="${GITHUB_REPO:-matrixNeo76/wasmbox}"

if ! command -v gh >/dev/null 2>&1; then
  echo "ERRORE: gh non disponibile — installa la GitHub CLI." >&2
  exit 1
fi

echo "Repo: $REPO"
echo "---"

# check <label> <path>: cattura l'output (stdout+stderr) in una variabile
# e classifica sul contenuto — senza file temporanei.
check() {
  label="$1"
  path="$2"
  ok="yes"
  resp="$(gh api "repos/$REPO/$path" 2>&1)" || ok="no"

  if [ "$ok" = "yes" ]; then
    total=$(printf '%s' "$resp" | grep -o '"total_count":[0-9]*' | head -1 | cut -d: -f2)
    echo "$label: HTTP 200 — alert aperti: ${total:-0}"
  elif printf '%s' "$resp" | grep -q "Resource not accessible by integration"; then
    echo "$label: HTTP 403 — la credenziale non può leggere gli alert (serve scope security_events)."
  elif printf '%s' "$resp" | grep -q "gh auth login"; then
    echo "$label: auth mancante per gh nel processo — imposta GH_TOKEN=… (per es.: GH_TOKEN=… sh ./scripts/ghas-status.sh) oppure esegui gh auth login"
  elif printf '%s' "$resp" | grep -q "Not Found"; then
    echo "$label: HTTP 404 — feature non abilitata (Settings → Code security) o repo assente"
  else
    echo "$label: errore — esegui: gh api repos/$REPO/$path"
  fi
}

check "Code scanning  " "code-scanning/alerts?state=open&per_page=1"
check "Secret scanning" "secret-scanning/alerts?state=open&per_page=1"
check "Dependabot     " "dependabot/alerts?state=open&per_page=1"
