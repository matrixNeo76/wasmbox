---
type: Log
title: "log — aggiornamenti documentazione wasmbox"
description: "Storia datata degli aggiornamenti del bundle di documentazione (solo eventi rilevanti per i documenti)."
okf_version: "0.2"
updated: "2026-10-10"
---

# Log — modifiche alla documentazione

## 2026-10-10 — 3 scenari eseguibili (integration.md portati in pratica)

- Richiesta utente: creare gli esempi dei 3 scenari della guida.
- **Scenario A** (`examples/scenario-cli-pipeline` + `scripts/scenarios/scenario-a.sh`):
  pipeline di fiducia — 4 case (normale/vuoto/100KB/non-UTF8) via
  `wasmbox-cli --json`, verdetto TRUST/REJECT, registro per SHA-256.
  Testato: TRUST + registrazione.
- **Scenario B** (`examples/scenario-tool-handler` con `guest/` dedicato):
  protocollo `tool:<nome>:<arg>` → `tool_result:…` con `HostHandler` reale,
  metriche post-run, errore host → exit 7. Testato: soma=42, reverse, len,
  boom→exit 7.
- **Scenario C** (`examples/scenario-hostile-guest` + runner): 3 bombe WAT
  (fuel/oom/ask-flood) compilate via `crates/wat-compile-scenarios` (crate
  `wat`, dev-dep già autorizzata). Testato 4 volte: fuel→exit 3 o 4
  (non deterministico: entrambi limiti validi, il runner li accetta),
  OOM→5, ask-flood→6. Zero crash.
- `docs/integration.md` §5 "Scenari provabili", `index.md` aggiornati;
  baseline completa verde (check/test 22/22/clippy/fmt).
- Workspace: 2 nuovi membri (scenario-tool-handler + guest,
  wat-compile-scenarios).

## 2026-10-10 — nuova guida d'integrazione (docs/integration.md)

- Richiesta utente: documento approfondito su come implementare wasmbox in
  altri sistemi, applicativi e agenti — il buco era confermato dalla
  ricerca (SKILL.md copre solo la via CLI e dichiara il proprio limite;
  host-run è un esempio senza spiegazione).
- Creato `docs/integration.md` (OKF v0.2, `type: Guide`): le tre vie
  (binaria/libreria/estensioni), mappa exit code CLI con politica per
  agente, `SandboxConfig` campo per campo con default reali dal sorgente,
  mappa completa `SandboxError` con politica host, protocollo consigliato
  sopra `ask` (request/response tipizzate lato host), template guest
  conforme + validazione con input ostile, checklist finale.
- `index.md` aggiornato (riga concept + ordine di lettura); il grafo nel
  preview raccoglierà i nuovi nodi/links automaticamente al prossimo
  render (scan on-the-fly dei link).

## 2026-10-10 — mappa dei concetti OKF come grafo nel preview

- Richiesta utente: indice OKF + grafo (vis.js/d3). Deciso col cliente:
  grafo nel preview report (`scripts/render_report.py`), generato al volo
  scansionando i link markdown reali — zero drift con il repo; niente
  `docs/graph.html` permanente (coerente con la decisione «no graphify»
  del 2026-10-08) ma la funzione `build_okf_graph` è riusabile per un
  evento standalone futuro.
- Verificato: 8 nodi (5 doc bundle + README/AGENTS/SKILL), 18 archi reali
  deduplicati; vis-network via CDN; JS inline e JSON grafo validati;
  preview gratuitato `freebuff-preview restart` → pagina servita con
  sezione grafo (`id="okf-graph"`) + screenshot UI (HTTP 200).

## 2026-10-10 — matrice multipiattaforma VERIFICATA in CI (tag v0.5.1)

- Tag `v0.5.1` spinto → workflow Release: **tutti e 4 i job verdi**
  (ubuntu-latest, macos-15, macos-15-intel, windows-2025).
- Release `v0.5.1` con gli **8 asset attesi**: 4 archivi con target-triple
  (3× tar.gz + 1× .zip su Windows) + 4 file .sha256.
- ROADMAP §2.0 aggiornato col riscontro reale: la matrice diventa la release
  standard per ogni tag successivo.

## 2026-10-10 — spec + piano per la portabilità multipiattaforma (v0.6)

- Richiesta utente: usare wasmbox in TUTTI gli ambienti, non solo Linux.
- `blueprint.md` → **v0.6**: nuova sezione "PORTABILITÀ MULTIPIATTAFORMA"
  con la matrice di release (4 runner/target triple: linux-gnu,
  aarch64-apple-darwin, x86_64-apple-darwin, x86_64-pc-windows-msvc), le
  regole del workflow a matrice, i rischi documentati e l'ordine di
  implementazione (incluso il tag di verifica `v0.5.1`).
- Codice Rust dichiarato INVARIATO (std-puro): cambia solo la superficie di
  rilascio (`.github/workflows/release.yml`, packaging `.tar.gz`/`.zip`,
  nomi asset con target-triple).
- ROADMAP §2.0 aggiornato: residuo "runner multipli" aggiornato con lo stato
  della spec e del piano.

## 2026-10-09 — release GitHub con binari scaricabili

- Nuovo workflow `.github/workflows/release.yml`: su tag `vX.Y.Z` builda
  release (wasmbox-cli, wasmbox-ui, guest_echo.wasm), esegue baseline +
  smoke test (CLI --json, screenshot BMP) e pubblica la release con tar.gz
  + SHA-256. PRIMA non esistevano release su GitHub (verificato: 0).
- README: sezione "Release GitHub" con istruzioni tag e uso dei binari.
  Limitazione dichiarata: solo linux x86_64 per ora.

## 2026-10-09 — interfacce di fruizione (CLI + skill + UI)

- Blueprint → **v0.5**: nuova sezione "INTERFACCE DI FRUIZIONE" con la spec
  di `wasmbox-cli` (exit code 0/2..10, `--json`), `wasmbox-ui`
  (screenshot BMP headless, senza systemfonts) e la skill agenti.
- ROADMAP §3 punto 10: cronologia della sessione con prove.
- README: quickstart CLI/UI, struttura workspace aggiornata (necessario),
  sezione "Cosa NON è" riscritta (CLI/skill/UI ora presenti; restano fuori
  endpoint HTTP e FFI).
- AGENTS.md §3: stato con CLI/skill/UI e preview aggiornato.
- `scripts/preview.sh` + `scripts/render_report.py`: report con demo CLI
  (output JSON reale) e screenshot UI embedded; fix stale "Wasmtime 28".
- Fix delivery preview (seguente): il BMP non è più incorporato inline
  (pagina 312 KB → 3 KB); la pagina referenzia `ui-screenshot.bmp` come
  file separato con link di download. Motivo: viewer/tagli scroll su pagine
  HTML troppo grandi — l'immagine era presente ma non raggiungibile.

## 2026-10-09 — audit di completamento + OKF v0.2

- Prima riga dell'indirizzo in `ROADMAP.md`, `blueprint.md`,
  `github-advanced-security.md`: indicati con precisione tutti i punti
  verificati (Wasmtime 49.0, 27/27 test, CI+CodeQL verdi, benchmark reali,
  LICENSE-MIT/APACHE, README + badge, `examples/host-run`, bench solo-std).
- Nuovo documento OKF: `index.md` (directory listing) e questo `log.md`.
- AGENTS.md aggiornato: era stantio (wasmtime 28, 20/20, pendenti risolti come
  aperti). Ora allineato: baseline 27/27, wasmtime 49.0, `gnario` superato,
  crates.io (`wasmbox` occupato, `wasmbox-core` libreto), divieto anyhow.

## 2026-10-09 — v0.4 (aggiunte post-validazione approvate)

- `blueprint.md` → v0.4: `examples/host-run`, `benches/perf.rs` (solo std),
  7 test di robustezza (→ 27 totali), metadati crates.io con
  `cargo publish --dry-run` che esce 0.

## 2026-10-08 — v0.3 (migrazione wasmtime 28 → 49.0)

- `blueprint.md` → v0.3: `anyhow` rimosso, host function su `wasmtime::Result`,
  `IntoFunc` non accetta più `anyhow::Result`, `PoolingAllocationConfig`
  in byte non pagine.

## 2026-10-08 — prima stesura progetto

- `blueprint.md` v0.2 salvato prima del codice (richiesta esplicita utente).
- `ROADMAP.md`: cronologia sessione-by-sessione con prove reali.
