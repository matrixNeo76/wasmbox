---
type: Concept
title: "Scenario A — pipeline editoriale via CLI (agente + policy di fiducia)"
description: "Simula un agente che riceve un guest generato, lo valida con wasmbox-cli --json su input normale E ostile, e decide se fidarlo (registro per SHA-256) o rifiutarlo."
resource: "examples/scenario-cli-pipeline/README.md"
tags: ["scenario", "cli", "agents", "policy", "validation"]
updated: "2026-10-10"
---

# Scenario A — pipeline editoriale via CLI

Simula un **agente AI che riceve un guest `.wasm` generato e decide se fidarlo**.
È la via 1 di [`docs/integration.md`](../../docs/integration.md): zero Rust lato
host, la decisione si prende da exit code + JSON di `wasmbox-cli`.

## Come si esegue

```sh
sh scripts/scenarios/scenario-a.sh            # usa guest-echo precompilato
sh scripts/scenarios/scenario-a.sh mio.wasm   # guest a scelta
```

Requisiti: `cargo build -p wasmbox-cli` e il guest wasm32 (compilato con
`sh scripts/build.sh`, oppure passa il tuo `.wasm` come argomento).

## Cosa fa (una policy realistica in 4 mosse)

1. **Run normale** con input valido: attesa `ok:true` ed `exit 0`.
2. **Input ostile** (3 casi): stringa vuota, stringa ×100 KB, byte non-UTF8 —
   atteso: `ok:true` (il guest è resiliente) **oppure** errore tipizzato
   (exit 3/4/5/6/10); **mai** crash del processo `wasmbox-cli`.
3. **Decisione**:
   - tutti i pass → **TRUST**: il guest viene archiviato come
     `registry/<sha256>.ok` (il riuso è istantaneo grazie alla cache `.cwasm`);
   - errore di risorsa (3/4/5/6) → **REJECT: rivedere il guest** (mai ritentare
     identico, come da SKILL.md);
   - errore di forma (8/9) → **REJECT: non conforme ABI**;
   - altro → **REJECT** con il messaggio ricevuto.
4. Verdetto su stdout come **una riga JSON** macchina-parsabile
   (`{"verdict":"trust","sha256":…}` / `{"verdict":"reject","reason":…}`),
   la convenzione di interoperabilità raccomandata tra agenti.

## Struttura

```
examples/scenario-cli-pipeline/
├── README.md     # questo file
└── registry/     # creato all'esecuzione: guest fidati, uno file per sha256
```

Lo scenario vive in `scripts/scenarios/scenario-a.sh` (nessun codice da scrivere
qui: è la dimostrazione che un agente decide solo con exit code + `--json`).
