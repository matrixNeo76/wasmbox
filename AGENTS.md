# AGENTS — leggi PRIMA di fare qualsiasi cosa

Questo progetto è **wasmbox**: un workspace Rust (**Wasmtime 49.0**) che esegue
codice WebAssembly non fidato con limiti di risorsa e una singola host function
opaca `ask`. Se stai aprendo il progetto per la prima volta (nuovo sandbox
Freebuff, nuova sessione), segui ESATTAMENTE questo ordine.

## 1. Ordine di lettura (obbligatorio)

1. [`docs/ROADMAP.md`](docs/ROADMAP.md) — **dove siamo**, cosa è fatto, cosa è
   pending, cronologia e trappole note (leggi §4 TUTTO, ti farà risparmiare ore).
2. [`docs/blueprint.md`](docs/blueprint.md) — **la spec vincolante** del prodotto
   (ABI, limiti, divieti, ordine di implementazione). Se blueprint e codice
   confliggono, il blueprint ha priorità.
3. [`docs/github-advanced-security.md`](docs/github-advanced-security.md) —
   stato integrazione GHAS.
4. [`docs/index.md`](docs/index.md) — indice di tutta la documentazione,
   in formato **OKF v0.2** (Open Knowledge Format).

## 2. Baseline — verifica PRIMA di modificare

```sh
sh ./scripts/install.sh                 # toolchain (idempotente)
cargo check --workspace                 # deve essere 0
cargo test -p wasmbox-core              # deve essere 27/27
cargo clippy --workspace --all-targets -- -D warnings   # 0
cargo fmt --all --check                 # 0
cargo build -p guest-echo --target wasm32-unknown-unknown --release
```

**Regola**: se la baseline non è verde, sistema quello prima di tutto. Non
"aggiustare" i test abbassando le asserzioni.

## 3. Stato corrente (sintesi — dettagli in ROADMAP)

- Prodotto: **completo e validato** (27/27 test, CI + CodeQL verdi, GHAS attivo).
- Repo GitHub canonico: `matrixNeo76/wasmbox` (pubblico, `main`, commit `7a65dfc`).
- Preview: `sh ./scripts/preview.sh` su porta 8080 (report dei test).
- Benchmark reali: compile freddo 1.355 ms, cache-hit 0.136 ms, overhead `ask` ~0.7 µs.
- **What wasmbox NON è ancora**: solo una **libreria** — non esiste CLI né
  endpoint HTTP/server per interrogarlo da un agente AI esterno; per il dettaglio
  vedi ROADMAP §2.9 (opzioni future, nessuna avviata).
- **OKF v0.2**: la `docs/` è in formato Open Knowledge Format v0.2 (frontmatter
  YAML su ogni concept; `index.md` + `log.md` riservati).
- **graphify / reactgraph**: DECISO di NON introdurli finché il repo resta
  piccolo (decisione dell'utente, 2026-10-08) — valutare se il repo cresce.
- Pendenti utente (uniche azioni esterne residue): **credito crates.io** per la
  pubblicazione reale di `wasmbox-core` (dry-run OK; nome libero — `wasmbox`
  puro, invece, è occupato dal 2022).

## 4. Divieti e insidie (riassunto — elenco completo in ROADMAP §4)

- **NIENTE WASI, `std::process::Command`, logica di dominio in `wasmbox-core`**,
  `serde_json`, `Box<dyn HostHandler>` in `run()`, thread per `run()`,
  `downcast_ref` senza `e.chain()`, `anyhow` (sostituito da `wasmtime::Result`).
  Solo due blocchi `unsafe` in `engine.rs` (transmute fat pointer + deref
  handler), entrambi documentati.
- CodeQL + Rust: solo `build-mode: none` (non `manual` — fallisce).
- **Git/gh verso GitHub dal sandbox: FUNZIONA** con la credenzia GitHub App
  gestita Freebuff (superato 2026-10-08 — trappola storica). `gh` **direct** OK,
  ma **dentro uno script non propaga la credenzia** e non ha scope di lettura
  alert (HTTP 403): per la verifica programmatica serve un token utente nel
  processo (`GH_TOKEN=… sh ./scripts/ghas-status.sh`) — dettagli in
  `docs/github-advanced-security.md`.
- Variabili `.env` **non** arrivano ai processi del terminale (bug noto).
- **crates.io**: il nome `wasmbox` è **occupato** (crate 2022,
  drifting-in-space/wasmbox) — non provocare conflitto: il nostro pacchetto è
  `wasmbox-core`.

## 5. Convenzioni

- Lingua dell'utente: **italiano**. Risposte concrete, con prove
  (exit code, HTTP code), mai affermazioni non verificate.
- Stile: Rust 2021, error messages in italiano (vedi `error.rs`), niente
  dipendenze oltre a wasmtime/thiserror (+ `wat` in dev).
- Ogni modifica non banale → rieseguire la baseline del §2 prima di dichiarare
  completato il lavoro.
