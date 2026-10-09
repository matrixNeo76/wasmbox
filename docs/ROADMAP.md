# wasmbox — ROADMAP & STATO DEL PROGETTO

> **Documento di continuità**: chiunque riprenda il lavoro (nuovo progetto Freebuff,
> nuovo agente, collaboratore) deve poter partire da qui. Spec tecnica completa:
> [`blueprint.md`](blueprint.md). Integrazione sicurezza:
> [`github-advanced-security.md`](github-advanced-security.md).
> Ultimo aggiornamento: 2026-10-08.

---

## 1. STATO ATTUALE (verificato, non stimato)

### 1.1 Prodotto — COMPLETO E VALIDATO ✅

Workspace Rust (Wasmtime 49) che esegue codice Wasm non fidato con limiti di
risorsa e una sola host function opaca `ask`.

```
crates/wasmbox-core/     # config, error, memory, engine + tests/e2e_test.rs
examples/guest-echo/     # guest wasm32 (cdylib)
docs/blueprint.md        # SPEC COMPLETA (vincolante)
scripts/                 # install, build, preview, ghas-status, push_via_api
.github/workflows/       # CodeQL (build-mode: none)
.github/dependabot.yml   # cargo + github-actions weekly
```

Validazione (2026-10-08, su wasmtime 49.0, tutti exit 0):

| Check | Comando | Esito |
|---|---|---|
| Typecheck | `cargo check --workspace` | 0 |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | 0 |
| Formattazione | `cargo fmt --all --check` | 0 |
| Test | `cargo test -p wasmbox-core` | 0 — **20/20** (5 unit + 15 e2e) |
| Guest wasm32 | `cargo build -p guest-echo --target wasm32-unknown-unknown --release` | 0 |
| E2E guest reale | `GUEST_ECHO_WASM=... cargo test ... guest_echo_e2e` | ok (non skippato) |

**Prima di toccare il codice**: rieseguire questi 5 comandi; sono la baseline.

### 1.2 GitHub — CANONICO E VERIFICATO ✅

- Repo: **`https://github.com/matrixNeo76/wasmbox`** (pubblico, branch `main`)
- Contenuto: **23 file, byte-identici alla copia locale** (verifica sha1 blob)
- La storia remota è **pulita** (3 commit iniziali via API, senza artefatti):
  se il git locale divergesse, il remoto è la fonte di verità
  (`git fetch && git reset --hard origin/main`)

### 1.3 CI / Sicurezza — ATTIVO ✅

| Componente | Stato | Note |
|---|---|---|
| CodeQL | ✅ run **success** | FIX applicato: Rust richiede `build-mode: none` (non `manual`) |
| Dependabot version updates | ✅ funzionante | 2 PR aperte: #2 `wasmtime 28 → 49.0.2` **adottata in locale e validata** (chiuderla dopo il commit della migrazione); #1 `actions/checkout 4 → 7` (bump sicuro, da mergiare) |
| Dependabot alerts | ✅ abilitati | 0 alert |
| Secret scanning + push protection | ✅ abilitati | via API |
| Actions | ✅ illimitate | repo pubblico |
| CodeRabbit | ❌ non installato | serve installazione manuale dal marketplace (OAuth utente) |

### 1.4 Preview Freebuff — CONFIGURATO

- `set-install`: `sh ./scripts/install.sh`
- `set-build`: `sh ./scripts/build.sh`
- `set`: `sh ./scripts/preview.sh` porta **8080** (esegue la suite e serve il report)

---

## 2. PROSSIMI PASSI (in ordine di priorità)

1. ~~**Riconnessione GitHub / nuovo progetto**~~ ✅ **RISOLTO** (verificato
   2026-10-08): il progetto corrente è agganciato a `matrixNeo76/wasmbox`
   (git remote corretto) e `git`/`gh` funzionano con la credenzia GitHub App
   gestita Freebuff (`gh auth status` → freebuff-web[bot]). Nessuna azione.
2. ~~**Riprendere il lavoro**~~ ✅ fatto: ROADMAP + `docs/blueprint.md` letti,
   baseline §1.1 rieseguita (5/5 verdi).
3. **Revocare il PAT** usato per i push — resta **solo azione manuale
   dell'utente** (richiede il login su GitHub, non eseguibile dall'agente):
   GitHub → Settings → Developer settings → Tokens (classic) → Revoke.
   Ora è superfluo: la credenzia gestita Freebuff sostituisce il PAT.
4. ~~**Eliminare `matrixNeo76/wasm-executor`**~~ ✅ **non esiste più**
   (verificato 2026-10-08: REST 404 + GraphQL "Could not resolve" con
   credenzia valida che vede gli altri repo dell'account). Nessuna azione.
5. ~~**Decidere sulla PR Dependabot `wasmtime 49`**~~ ✅ **DECISO: adottata e
   validata** (2026-10-08). Migrazione completa in workspace: `Cargo.toml`
   (`28.0` → `49.0`), `engine.rs` (host function e `map_guest_error` su
   `wasmtime::Result`/`wasmtime::Error` — `IntoFunc` in 49 non accetta più
   `anyhow::Result`), dipendenza `anyhow` rimossa, blueprint → v0.3.
   Baseline 5/5 verde su 49: check, **20/20 test** (guest e2e reale incluso),
   clippy `-D warnings`, fmt, guest wasm32. Resta all'utente:
   - salvare/commitare i cambi dal pannello Changes di Freebuff;
   - **chiudere la PR #2** (toca solo Cargo.toml/lock: da sola romperebbe `main`);
   - mergiare la PR #1 `actions/checkout 4 → 7` (bump sicuro).
6. **CodeRabbit** (facoltativo): installare dal GitHub Marketplace sul repo `wasmbox`.
7. **`graphify` / `reactgraph` — DECISO: differiti** (decisione utente 2026-10-08):
   l'utente li usa di norma per leggere codice/UI, ma il repo è piccolo e
   autoesplicativo → **non introdurli** finché non serve; rivalutare se il
   repo cresce. (Nessuna traccia nel workspace: grep=0, zero dipendenze;
   rieseguito 2026-10-08 → sempre 0.)
8. **Segnalazione bug Freebuff** (facoltativa): aprire issue su
   `CodebuffAI/freebuff` sul nome repo bloccato (issue correlate note: #1403, #1423).

---

## 3. COSA È STATO FATTO IN QUESTA SESSIONE (cronologia sintetica)

1. Blueprint salvato in `docs/blueprint.md` (richiesta esplicita: salvarlo come .md).
2. Workspace costruito secondo il blueprint (7 fasi) + 3 adattamenti ai fatti reali:
   - `total_stacks` assente senza feature `async` → rimosso dal pooling;
   - `#[link(wasm_import_module = "env")]` necessario altrimenti `rust-lld`
     fallisce con `undefined symbol: ask`;
   - `guest_free`/`guest_run` → `pub unsafe extern "C"` + sezioni `# Safety`
     (clippy `not_unsafe_ptr_arg_deref` / `missing_safety_doc`); firma wasm invariata.
3. Suite test creata (15 e2e + 5 unit), tutto verde.
4. Preview Freebuff configurato (report dei test servito su 8080).
5. Integrazione GitHub Advanced Security (workflow CodeQL, dependabot, doc, script).
6. Saga GitHub: init/commit/fix branch (`master`→`main`)/fix remote → il pannello
   Freebuff falliva a ogni passo; risolto via **GitHub REST API**
   (`scripts/push_via_api.py`, seed via Contents API per il repo vuoto, poi
   blob/tree/commit/ref) → 23 file su `wasmbox`, repo reso pubblico.
7. README creato e pushato; CodeQL fixato (`build-mode: none`) — run verde.
8. Abilitati secret scanning, push protection, dependabot alerts (API).
9. Ricerca Freebuff: nessun annuncio pubblico di deprecazione; issue note #1403/#1423.
10. Sessione 2026-10-08 (ripresa): baseline 5/5 su wasmtime 28 → upgrade a
    **wasmtime 49.0** valutato e adottato (2 soli punti di rottura: firma
    `IntoFunc` che vuole `wasmtime::Result` e `guest_run.call` che restituisce
    `wasmtime::Error`) → 5/5 verdi con 20/20 test; blueprint → v0.3, README
    aggiornato a Wasmtime 49; `wasm-executor` confermato inesistente (404);
    `git`/`gh` confermati funzionanti con la credenzia gestita Freebuff.

---

## 4. TRAPPOLE CONOSCIUTE (non perdere tempo a riscoprirle)

| # | Trappola | Fatto |
|---|---|---|
| 1 | ~~Gate piattaforma~~ **SUPERATO 2026-10-08**: `git` e `gh` ora funzionano (credenzia GitHub App gestita Freebuff, iniettata automaticamente; `gh auth status` → freebuff-web[bot]) | `scripts/push_via_api.py` resta solo come fallback storico |
| 2 | `.env`/Keys **non propagano** i valori ai processi del terminale (`GITHUB_TOKEN` = lunghezza 0 sempre) | passare il token via stdin in chat, non da `.env` |
| 3 | Pannello Settings: nome repo bloccato sul nome progetto (`wasm-executor`) | nuovo progetto (vedi §2.1) |
| 4 | Pannello "save version" → errore `repo_not_connected` finché non si ricollega | riconnessione da Settings, oppure lavorare sul nuovo progetto |
| 5 | CodeQL + Rust: `manual` build mode **non supportato** | `build-mode: none` (gia nel workflow) |
| 6 | Git database API su repo **vuoto** → `409 Git Repository is empty` | seed del primo file via Contents API (già nello script) |
| 7 | `wasmbox` privato visibile solo col token; anonimo → 404 (non è inesistente) | ora è pubblico, anonimo → 200 |

---

## 5. CONTESTO PER IL PROSSIMO AGENTE

- **Spec**: `docs/blueprint.md` è la fonte autoritativa del "cosa deve fare".
  Questo file è il "dove siamo".
- **Comandi rapidi**: `sh ./scripts/install.sh` (toolchain — **`cargo` non è
  preinstallato**: va eseguito prima di ogni comando di baseline), `sh ./scripts/build.sh`
  (check + guest wasm), `sh ./scripts/preview.sh` (test + report).
- **Test opzionale guest reale**:
  `GUEST_ECHO_WASM=target/wasm32-unknown-unknown/release/guest_echo.wasm cargo test -p wasmbox-core`
- **Nessun servizio esterno richiesto** dal prodotto (niente DB, auth, email).
- **Lingua dell'utente**: italiano. Preferisce risposte concrete con prove
  (HTTP code, exit code) e si è arrabbiato per istruzioni a "pannelli" che
  non funzionavano → **verificare prima, consigliare poi**.
