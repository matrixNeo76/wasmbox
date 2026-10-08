# GitHub Advanced Security — integrazione `wasmbox`

Questa integrazione abilita le funzionalità di sicurezza di GitHub sul
repository: **CodeQL code scanning**, **secret scanning** e **Dependabot**.

## Cosa è stato aggiunto al progetto

| File | Ruolo |
|---|---|
| `.github/workflows/codeql.yml` | Analisi CodeQL (linguaggio `rust`) su push, PR e scan settimanale; carica gli alert in GitHub → Security → Code scanning |
| `.github/dependabot.yml` | Aggiornamenti settimanali delle dipendenze `cargo` e delle GitHub Actions (PR automatiche) |
| `scripts/ghas-status.sh` | Verifica via API dello stato delle tre funzionalità usando `GITHUB_TOKEN` (senza mai stamparlo) |

## Chiave da inserire nella tab **Keys** del progetto

| Nome chiave | Valore |
|---|---|
| `GITHUB_TOKEN` | Un **Personal Access Token classico con scope `repo`** (sufficiente per leggere code-scanning/secret-scanning/Dependabot alerts e per le operazioni su repo) |

Crea il token da: GitHub → Settings → Developer settings → Personal access
tokens → Tokens (classic) → Generate new token → scope **`repo`** → incolla
il valore nella tab Keys del progetto (nessun'altra chiave è necessaria).

## Abilitazione lato GitHub (una tantum, dopo il push del repo)

1. **Code scanning**: attivo di default appena il workflow CodeQL viene
   eseguito (tab Actions del repo, poi Security → Code scanning).
2. **Secret scanning + push protection**: Settings → Code security and
   analysis → abilita *Secret scanning* e *Push protection*.
3. **Dependabot alerts**: Settings → Code security and analysis → abilita
   *Dependabot alerts* (i PR li apre automaticamente grazie a
   `.github/dependabot.yml`).

**Costi/licenze**: su **repo pubblici** code scanning, secret scanning e
Dependabot sono **gratuiti**. Su repo **privati** richiedono una licenza
GitHub Advanced Security (GHAS); senza licenza il workflow CodeQL restituirà
404/403 sugli alert — segnale che serve l'abbonamento o che il repo deve
essere pubblico.

## Verifica

```sh
sh ./scripts/ghas-status.sh
```

Stampa HTTP code e conteggio degli alert aperti per ciascuna delle tre
funzionalità. `404` = feature non abilitata o repo assente; `401` = token da
rigenerare; `200` = integrazione attiva.

**Prerequisito**: il repository deve essere pushato su GitHub
(`matrixNeo76/wasmbox`) — il workflow parte al primo push.
