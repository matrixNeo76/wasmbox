---
type: Log
title: "log — aggiornamenti documentazione wasmbox"
description: "Storia datata degli aggiornamenti del bundle di documentazione (solo eventi rilevanti per i documenti)."
okf_version: "0.2"
updated: "2026-10-09"
---

# Log — modifiche alla documentazione

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
