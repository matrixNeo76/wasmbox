# GitHub Advanced Security — integrazione `wasmbox`

Questa integrazione abilita le funzionalità di sicurezza di GitHub sul
repository: **CodeQL code scanning**, **secret scanning** e **Dependabot**.

## Cosa è stato aggiunto al progetto

| File | Ruolo |
|---|---|
| `.github/workflows/codeql.yml` | Analisi CodeQL (linguaggio `rust`) su push, PR e scan settimanale; carica gli alert in GitHub → Security → Code scanning |
| `.github/dependabot.yml` | Aggiornamenti settimanali delle dipendenze `cargo` e delle GitHub Actions (PR automatiche) |
| `scripts/ghas-status.sh` | Verifica via `gh api` lo stato delle tre funzionalità — richiede un token nel processo (`GH_TOKEN=… sh ./scripts/ghas-status.sh`); non stampa mai il token |

## Come eseguire la verifica (aggiornato 2026-10-09, con prove)

La credenziale GitHub App gestita funziona per `git`/`gh` **diretti**, ma:
- **non si propaga** ai sottoprocessi di uno script (gh risponde "gh auth login");
- **non ha il permesso** di lettura degli alert (HTTP 403 "Resource not
  accessible by integration" sulle tre API alert).

Per la verifica programmatica dei tre tipi d'alert serve quindi un **token
utente** con permesso di lettura degli alert, passato come variabile di
processo — mai su riga di comando come argomento:

```sh
GH_TOKEN=ghp_… sh ./scripts/ghas-status.sh
```

Oppure, senza token: apri gli alert nel browser su
**GitHub → repo → Security** (code scanning, secret scanning, Dependabot).

## Abilitazione lato GitHub (una tantum, dopo il push del repo)

1. **Code scanning**: attivo di default appena il workflow CodeQL viene
   eseguito (tab Actions del repo, poi Security → Code scanning).
2. **Secret scanning + push protection**: Settings → Code security and
   analysis → abilita *Secret scanning* e *Push protection*.
3. **Dependabot alerts**: Settings → Code security and analysis → abilita
   *Dependabot alerts* (i PR li apre automaticamente grazie a
   `.github/dependabot.yml`).

> Aggiornamento 2026-10-09: su questo repo **tutte e tre le feature sono
> già attive** (verificate: 0 alert). L'abilitazione qui sopra resta come
> riferimento storico.

**Costi/licenze**: su **repo pubblici** code scanning, secret scanning e
Dependabot sono **gratuiti**. Su repo **privati** richiedono una licenza
GitHub Advanced Security (GHAS); senza licenza il workflow CodeQL restituirà
404/403 sugli alert — segnale che serve l'abbonamento o che il repo deve
essere pubblico.

## Verifica

```sh
GH_TOKEN=… sh ./scripts/ghas-status.sh
```

Stampa il conteggio degli alert aperti per ciascuna delle tre funzionalità.
Senza token nel processo, lo script riporta chiaramente "auth mancante per
gh nel processo" invece di fallire in silenzio.

**Prerequisito**: il repository deve essere pushato su GitHub
(`matrixNeo76/wasmbox`) — il workflow parte al primo push.
