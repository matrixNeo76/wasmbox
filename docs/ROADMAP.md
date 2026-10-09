# wasmbox — ROADMAP & STATO DEL PROGETTO

> **Documento di continuità**: chiunque riprenda il lavoro (nuovo progetto Freebuff,
> nuovo agente, collaboratore) deve poter partire da qui. Spec tecnica completa:
> [`blueprint.md`](blueprint.md). Integrazione sicurezza:
> [`github-advanced-security.md`](github-advanced-security.md).
> Ultimo aggiornamento: 2026-10-09.

---

## 1. STATO ATTUALE (verificato, non stimato)

### 1.1 Prodotto — COMPLETO E VALIDATO ✅

Workspace Rust (Wasmtime 49) che esegue codice Wasm non fidato con limiti di
risorsa e una sola host function opaca `ask`.

```
crates/wasmbox-core/     # config, error, memory, engine + tests/e2e_test.rs + benches/perf.rs
examples/guest-echo/     # guest wasm32 (cdylib)
examples/host-run/       # esempio host: carica un guest e gli passa un HostHandler
docs/blueprint.md        # SPEC COMPLETA (vincolante, v0.4)
scripts/                 # install, build, preview, ghas-status, push_via_api
.github/workflows/       # CI (baseline: check/test/clippy/fmt/guest) + CodeQL
.github/dependabot.yml   # cargo + github-actions weekly
LICENSE-MIT, LICENSE-APACHE
```

Validazione (2026-10-09, su wasmtime 49.0, tutti exit 0):

| Check | Comando | Esito |
|---|---|---|
| Typecheck | `cargo check --workspace` | 0 |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | 0 |
| Formattazione | `cargo fmt --all --check` | 0 |
| Test | `cargo test -p wasmbox-core` | 0 — **27/27** (5 unit + 22 e2e) |
| Guest wasm32 | `cargo build -p guest-echo --target wasm32-unknown-unknown --release` | 0 |
| E2E guest reale | `GUEST_ECHO_WASM=... cargo test ... guest_echo_e2e` | ok (non skippato) |

**Prima di toccare il codice**: rieseguire questi 5 comandi; sono la baseline.

### 1.2 GitHub — CANONICO E VERIFICATO ✅

- Repo: **`https://github.com/matrixNeo76/wasmbox`** (pubblico, branch `main`)
- Contenuto: **25 file, byte-identici alla copia locale** (verifica sha1 blob;
  `git ls-files | wc -l` = 25)
- La storia remota è **pulita** (commit initiali via API + migrazione wasmtime 49
  spinta il 2026-10-09, `c2963d7`): se il git locale divergesse, il remoto è la
  fonte di verità (`git fetch && git reset --hard origin/main`)

### 1.3 CI / Sicurezza — ATTIVO ✅

| Componente | Stato | Note |
|---|---|---|
| CodeQL | ✅ run **success** | FIX applicato: Rust richiede `build-mode: none` (non `manual`) |
| CI baseline | ✅ **aggiunta 2026-10-09** | `.github/workflows/ci.yml`: check, **test (con `GUEST_ECHO_WASM` reale)**, clippy `-D warnings`, fmt, guest wasm32, su push/PR. Prima **nessuna CI compilava/testava** (CodeQL è solo analisi statica) |
| Dependabot version updates | ✅ funzionante | **0 PR aperte** (2026-10-09): #2 `wasmtime 28 → 49.0.2` **chiusa** (supersita dalla migrazione locale spinta in `c2963d7`); #1 `actions/checkout 4 → 7` **mergiata** (commit `0795a37`) |
| Dependabot alerts | ✅ abilitati | 0 alert |
| Lettura alert via API | ⚠️ **verificato 2026-10-09** | La credenziale gestita NON si propaga ai sottoprocessi (`gh` dentro script chiede "gh auth login") e ha scope limitato (HTTP 403 "Resource not accessible by integration" sulle alert). Per la lettura programmatica: `GH_TOKEN=… sh ./scripts/ghas-status.sh` con token utente 
| Secret scanning + push protection | ✅ abilitati | via API |
| Actions | ✅ illimitate | repo pubblico |
| Licenze | ✅ **aggiunte 2026-10-09** | `LICENSE-MIT` + `LICENSE-APACHE` (prima mancavano, benché `Cargo.toml` dichiari `MIT OR Apache-2.0`) |
| CodeRabbit | ✅ **integrato** | App già installata dall'utente su tutti i suoi repo pubblici; badge nel README (verificato 2026-10-09) |

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
3. ~~**Revocare il PAT**~~ **DECISO: NON CRITICO** (decisione utente
   2026-10-09): il PAT resta attivo di proposito — l'utente lo vuole
   disponibile per le sessioni agente. Nessuna azione programmatata.
4. ~~**Eliminare `matrixNeo76/wasm-executor`**~~ ✅ **non esiste più**
   (verificato 2026-10-08: REST 404 + GraphQL "Could not resolve" con
   credenzia valida che vede gli altri repo dell'account). Nessuna azione.
5. ~~**Decidere sulla PR Dependabot `wasmtime 49`**~~ ✅ **DECISO: adottata e
   validata** (2026-10-08). Migrazione completa in workspace: `Cargo.toml`
   (`28.0` → `49.0`), `engine.rs` (host function e `map_guest_error` su
   `wasmtime::Result`/`wasmtime::Error` — `IntoFunc` in 49 non accetta più
   `anyhow::Result`), dipendenza `anyhow` rimossa, blueprint → v0.3.
   Baseline 5/5 verde su 49: check, **20/20 test** (guest e2e reale incluso),
   clippy `-D warnings`, fmt, guest wasm32. **Completato il 2026-10-09**:
   - cambi salvati e spinti dal pannello Changes (commit `c2963d7`);
   - **PR #2 chiusa** con commento di supersessione (da sola avrebbe rotto `main`);
   - **PR #1 mergiata** (`0795a37`); CodeQL verde su entrambi i push.
6. ~~**CodeRabbit**~~ ✅ **GIÀ INTEGRATO** (2026-10-09): l'utente ha l'App
   installata su tutti i suoi repo pubblici; badge `coderabbit/prs` aggiunto
   al README. Nessuna azione.
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
11. Sessione 2026-10-09: baseline 5/5 rieseguita → rimosso `std::process::id()`
    da `engine.rs` (nome tmp cache via timestamp ns: la grep letterale del
    blueprint "zero `std::process`" ora passa) → preview riconfigurato
    (`install`/`build`/`set ... 8080`) e verificato `ready` con report PASS;
    **PR #2 chiusa, PR #1 mergiata** (credenzia gestita Freebuff),
    `git pull --rebase` + **push `c2963d7`**, CodeQL `success` su merge e push;
    Roadmap allineata allo stato reale; badge CodeRabbit aggiunto al README
    (l'App risulta già installata sui repo dell'utente).
12. Sessione 2026-10-09 (seguito): analisi dei gap reali → aggiunti
    `.github/workflows/ci.yml` (la vecchia CodeQL in `build-mode: none` NON
    compilava né testava: la baseline girava solo in locale) e i file
    `LICENSE-MIT` / `LICENSE-APACHE` (mancanti nonostante la doppia licenza
    dichiarata). Entrambi i punti approvati dall'utente come prioritari.
13. Sessione 2026-10-09 (seguito 2 — punti D+C+E+F approvati):
    - **D**: 7 test di robustezza (boundary `== max`/`max+1` su richiesta e
      risposta, `memory.grow` oltre il limite ⇒ -1, memoria iniziale oversize
      ⇒ `InvalidWasm`, 4 thread su stesso `cache_dir`) → **27/27**;
    - **C**: `examples/host-run` (membro workspace, run reale verificata:
      `echo_result:INSPECT:CIAO DA WASMBOX`);
    - **E**: `benches/perf.rs` con harness=false e **solo std** (nessuna
      dipendenza) — numeri reali: compile freddo 1.355 ms, cache-hit 0.136 ms
      (~10×), overhead round-trip `ask` 0.7 µs;
    - **F**: metadati crates.io + `cargo publish --dry-run` → **exit 0**
      (12 file, 86.1 KiB);
    - blueprint → **v0.4**.

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
