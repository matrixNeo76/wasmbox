---
type: Collection
title: "wasmbox — indice documentazione (OKF v0.2)"
description: "Directory listing di docs/: ogni concept in formato Open Knowledge Format v0.2 (frontmatter YAML, campo type obbligatorio)."
okf_version: "0.2"
updated: "2026-10-11"
---

# Indice documentazione `docs/`

Bundle OKF v0.2 ([spec](https://github.com/GoogleCloudPlatform/open-knowledge-format/blob/main/SPEC.md)):
ogni file concept è Markdown UTF-8 con frontmatter YAML (unico campo
obbligatorio: `type`). `index.md` e `log.md` sono riservati.

## Concetti

| File | `type` | Contenuto |
|---|---|---|
| [blueprint.md](blueprint.md) | Specification | Spec vincolante: obiettivo, ABI, limiti, divieti, interfacce di fruizione (v0.6 con matrice release 4 target), test, validazione |
| [integration.md](integration.md) | Guide | Guida all'integrazione: quattro vie d'uso (CLI per agenti, libreria per applicativi Rust, HTTP `wasmbox-http`, FFI `wasmbox-ffi` + tool LLM opzionale), SandboxConfig campo per campo, mappa errori, protocollo su `ask`, authoring guest, 6 scenari provabili (A–F), checklist |
| [ROADMAP.md](ROADMAP.md) | Specification | Dove siamo, prossimi passi, cronologia completa, trappole note, contesto per il prossimo agente |
| [github-advanced-security.md](github-advanced-security.md) | Reference | Stato CodeQL / secret scanning / Dependabot, procedure di verifica e abilitazione |
| [extension-plan.md](extension-plan.md) | Specification | Piano v0.7: M1 fuel-only, scenario D (HTTP), E (FFI/C-ABI), F (tool LLM via OpenRouter) — spec, ABI, prove |
| [wasi-plan.md](wasi-plan.md) | Specification | Piano v0.8 WASI opt-in — **IMPLEMENTATO 2026-10-11**: crate `wasmbox-wasi` separato dal core, capability deny-by-default, strict import check, stdout captured, deviazioni dichiarate in §10, 7/7 test verdi |
| [comparative-analysis.md](comparative-analysis.md) | Reference | Analisi comparativa fact-checked (v0.2, 2026-10-11): tassonomia dei 3 livelli di sandboxing (microVM / OS-level / in-process Wasm), matrice vs Firecracker, E2B, gVisor, Sandlock, Extism, RLBox, Eryx, sandboxd; threat model Spectre; caso studio CVE-2026-35195 (realloc Component Model) e come wasmbox lo chiude per design (allocatore cooperativo + bounds check); roadmap di consolidamento v0.2→v1.0 |
| [scenarios.md](scenarios.md) | Guide | Scenari d'integrazione via `ask`: 6 ricette complete (FastAPI ask-proxy, gRPC bridge, LLM tool-bus, CLI/subprocess, serverless, FFI in-process) con architettura, codice, limiti e checklist per ciascuno |

## Fuori da `docs/` ma parte del bundle

| File | Ruolo |
|---|---|
| [`../AGENTS.md`](../AGENTS.md) | Istruzioni operative per agenti (lettura obbligatoria al primo avvio) |
| [`../README.md`](../README.md) | Presentazione progetto + quickstart (CLI/UI incluse) |
| [`../skills/wasmbox/SKILL.md`](../skills/wasmbox/SKILL.md) | Skill per agenti AI: come usare `wasmbox-cli` (exit code, `--json`) |
| [`../crates/wasmbox-cli/`](../crates/wasmbox-cli/) | CLI per umani e agenti AI (run + `--json`) |
| [`../crates/wasmbox-ui/`](../crates/wasmbox-ui/) | UI minimale Slint con screenshot BMP headless |
| [`../.github/workflows/ci.yml`](../.github/workflows/ci.yml) | CI baseline (check/test/clippy/fmt/guest) + job benchmark (cargo bench con artifact pubblico) |
| [`../crates/wasmbox-wasi/`](../crates/wasmbox-wasi/) | WASI opt-in: Preview 1 subset capability-gated sopra wasmbox-core |
| [`../.github/workflows/codeql.yml`](../.github/workflows/codeql.yml) | CodeQL build-mode none |
| [`../examples/scenario-cli-pipeline/`](../examples/scenario-cli-pipeline/) | Scenario A: pipeline fiducia via CLI |
| [`../examples/scenario-tool-handler/`](../examples/scenario-tool-handler/) | Scenario B: handler tool via libreria |
| [`../examples/scenario-hostile-guest/`](../examples/scenario-hostile-guest/) | Scenario C: bombe ostili (limiti) |
| [`../crates/wasmbox-http/`](../crates/wasmbox-http/) | Scenario D: endpoint HTTP `POST /run` (orchestrazione remota) |
| [`../crates/wasmbox-ffi/`](../crates/wasmbox-ffi/) | Scenario E: C-ABI stabile (cdylib/staticlib) per host non-Rust |
| [`../scripts/scenarios/`](../scripts/scenarios/) | runner + build degli scenari |
| [`../scripts/`](../scripts/) | install / build / preview (con demo CLI+UI) / ghas-status |

## Ordine di lettura consigliato (per un nuovo agente)

1. [`../AGENTS.md`](../AGENTS.md) → regole operative immediate.
2. [ROADMAP.md](ROADMAP.md) → stato reale (§1 verificato, §4 trappole).
3. [blueprint.md](blueprint.md) → spec tecnica vincolante.
4. [integration.md](integration.md) → per chi deve integrare wasmbox in un
   sistema applicativo o usarla da agente AI (quali vie, come, a quali
   condizioni).
4b. [scenarios.md](scenarios.md) → ricette concrete per i casi d'uso più
   diffusi (FastAPI, gRPC, agenti LLM, serverless, FFI…): una volta scelta
   la via con integration.md, qui c'è il passo-passo.
4c. [comparative-analysis.md](comparative-analysis.md) → dove si colloca
   wasmbox rispetto alle altre sandbox e quali criticità di design sono
   già chiuse nel codice vs quali restano aperte (spec ABI, benchmark,
   out-of-process).
5. [github-advanced-security.md](github-advanced-security.md) → solo se si
   lavora su CI/sicurezza.
