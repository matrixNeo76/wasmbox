# AGENTS — leggi PRIMA di fare qualsiasi cosa

Questo progetto è **wasmbox**: un workspace Rust (Wasmtime 28) che esegue codice
WebAssembly non fidato con limiti di risorsa e una singola host function opaca
`ask`. Se stai aprendo il progetto per la prima volta (nuovo sandbox Freebuff,
nuova sessione), segui ESATTAMENTE questo ordine.

## 1. Ordine di lettura (obbligatorio)

1. [`docs/ROADMAP.md`](docs/ROADMAP.md) — **dove siamo**, cosa è fatto, cosa è
   pending, cronologia e trappole note (leggi §4 TUTTO, ti farà risparmiare ore).
2. [`docs/blueprint.md`](docs/blueprint.md) — **la spec vincolante** del prodotto
   (ABI, limiti, divieti, ordine di implementazione). Se blueprint e codice
   confliggono, il blueprint ha priorità.
3. [`docs/github-advanced-security.md`](docs/github-advanced-security.md) —
   stato integrazione GHAS.

## 2. Baseline — verifica PRIMA di modificare

```sh
sh ./scripts/install.sh                 # toolchain (idempotente)
cargo check --workspace                 # deve essere 0
cargo test -p wasmbox-core              # deve essere 20/20
cargo clippy --workspace --all-targets -- -D warnings   # 0
cargo fmt --all --check                 # 0
cargo build -p guest-echo --target wasm32-unknown-unknown --release
```

**Regola**: se la baseline non è verde, sistema quello prima di tutto. Non
"aggiustare" i test abbassando le asserzioni.

## 3. Stato corrente (sintesi — dettagli in ROADMAP)

- Prodotto: **completo e validato** (20/20 test, CI CodeQL verde, GHAS attivo).
- Repo GitHub canonico: `matrixNeo76/wasmbox` (pubblico, `main`, 25 file).
- Preview: `sh ./scripts/preview.sh` su porta 8080 (report dei test).
- **graphify / reactgraph**: DECISO di NON introdurli finché il repo resta
  piccolo (decisione dell'utente, 2026-10-08) — valutare se il repo cresce.
- Pendenti utente: riconnessione GitHub del progetto Freebuff, revoca PAT,
  eliminazione repo `wasm-executor`, PR Dependabot wasmtime 28→49 (da decidere).

## 4. Divieti e insidie (riassunto — elenco completo in ROADMAP §4)

- **NIENTE WASI, `std::process::Command`, logica di dominio in `wasmbox-core`**,
  `serde_json`, `Box<dyn HostHandler>` in `run()`, thread per `run()`,
  `downcast_ref` senza `e.chain()`. Solo due blocchi `unsafe` in `engine.rs`
  (transmute fat pointer + deref handler), entrambi documentati.
- CodeQL + Rust: solo `build-mode: none` (non `manual` — fallisce).
- Git verso GitHub dal sandbox: **bloccato dal gate Freebuff** → usare
  `printf '%s' "<token>" | python3 scripts/push_via_api.py matrixNeo76/wasmbox main`.
- Variabili `.env` **non** arrivano ai processi del terminale (bug noto).

## 5. Convenzioni

- Lingua dell'utente: **italiano**. Risposte concrete, con prove
  (exit code, HTTP code), mai affermazioni non verificate.
- Stile: Rust 2021, error messages in italiano (vedi `error.rs`), niente
  dipendenze oltre a wasmtime/thiserror/anyhow (+ `wat` in dev).
- Ogni modifica non banale → rieseguire la baseline del §2 prima di dichiarare
  completato il lavoro.
