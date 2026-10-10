---
type: Collection
title: "wasmbox — indice documentazione (OKF v0.2)"
description: "Directory listing di docs/: ogni concept in formato Open Knowledge Format v0.2 (frontmatter YAML, campo type obbligatorio)."
okf_version: "0.2"
updated: "2026-10-10"
---

# Indice documentazione `docs/`

Bundle OKF v0.2 ([spec](https://github.com/GoogleCloudPlatform/open-knowledge-format/blob/main/SPEC.md)):
ogni file concept è Markdown UTF-8 con frontmatter YAML (unico campo
obbligatorio: `type`). `index.md` e `log.md` sono riservati.

## Concetti

| File | `type` | Contenuto |
|---|---|---|
| [blueprint.md](blueprint.md) | Specification | Spec vincolante: obiettivo, ABI, limiti, divieti, interfacce di fruizione (v0.6 con matrice release 4 target), test, validazione |
| [integration.md](integration.md) | Guide | Guida all'integrazione: tre vie d'uso (CLI per agenti, libreria per applicativi Rust, estensioni future), SandboxConfig campo per campo, mappa errori, protocollo su `ask`, authoring guest, checklist |
| [ROADMAP.md](ROADMAP.md) | Specification | Dove siamo, prossimi passi, cronologia completa, trappole note, contesto per il prossimo agente |
| [github-advanced-security.md](github-advanced-security.md) | Reference | Stato CodeQL / secret scanning / Dependabot, procedure di verifica e abilitazione |

## Fuori da `docs/` ma parte del bundle

| File | Ruolo |
|---|---|
| [`../AGENTS.md`](../AGENTS.md) | Istruzioni operative per agenti (lettura obbligatoria al primo avvio) |
| [`../README.md`](../README.md) | Presentazione progetto + quickstart (CLI/UI incluse) |
| [`../skills/wasmbox/SKILL.md`](../skills/wasmbox/SKILL.md) | Skill per agenti AI: come usare `wasmbox-cli` (exit code, `--json`) |
| [`../crates/wasmbox-cli/`](../crates/wasmbox-cli/) | CLI per umani e agenti AI (run + `--json`) |
| [`../crates/wasmbox-ui/`](../crates/wasmbox-ui/) | UI minimale Slint con screenshot BMP headless |
| [`../.github/workflows/ci.yml`](../.github/workflows/ci.yml) | CI baseline (check/test/clippy/fmt/guest) |
| [`../.github/workflows/codeql.yml`](../.github/workflows/codeql.yml) | CodeQL build-mode none |
| [`../scripts/`](../scripts/) | install / build / preview (con demo CLI+UI) / ghas-status |

## Ordine di lettura consigliato (per un nuovo agente)

1. [`../AGENTS.md`](../AGENTS.md) → regole operative immediate.
2. [ROADMAP.md](ROADMAP.md) → stato reale (§1 verificato, §4 trappole).
3. [blueprint.md](blueprint.md) → spec tecnica vincolante.
4. [integration.md](integration.md) → per chi deve integrare wasmbox in un
   sistema applicativo o usarla da agente AI (quali vie, come, a quali
   condizioni).
5. [github-advanced-security.md](github-advanced-security.md) → solo se si
   lavora su CI/sicurezza.
