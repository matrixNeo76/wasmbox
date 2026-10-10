---
type: Concept
title: "Scenario B — handler tool reale via libreria (dominio nell'host)"
description: "Applicativo Rust con HostHandler che decodifica richieste tool:<nome>:<arg> dal guest via ask, esegue i tool lato host, risponde tool_result:… e mostra le metriche di sessione post-run."
resource: "examples/scenario-tool-handler/README.md"
tags: ["scenario", "library", "host-handler", "ask-protocol", "tools"]
updated: "2026-10-10"
---

# Scenario B — handler tool reale via libreria

Dimostra la **via 2** di [`docs/integration.md`](../../docs/integration.md):
un applicativo Rust usa `wasmbox-core` e colloca la logica di dominio
**(i "tool") interamente nell'host**. Il guest è solo un trasporto opaco
del protocollo `tool:<nome>:<argomento>` → `tool_result:<…>`.

## Come si esegue

```sh
sh scripts/scenarios/build-scenarios.sh          # build host + guest wasm32
cargo run -p scenario-tool-handler -- \
  target/wasm32-unknown-unknown/release/scenario_tool_guest.wasm "soma:7x35"
```

Requisiti: toolchain Rust con target `wasm32-unknown-unknown`.

## Cosa mostra

| Aspetto della guida (§2) | Come appare qui |
|---|---|
| `HostHandler` con dominio reale | `ToolHandler` con 3 tool puri (`soma`, `reverse`, `len`) |
| Protocollo sopra `ask` | richiesta `tool:<nome>:<arg>` → risposta `tool_result:<…>` (bytes opachi per il core) |
| Configurazione esplicita | `SandboxConfig { cache_dir, max_ask_calls, ..Default }` |
| Errore lato host tipizzato | tool `boom` → `SandboxError::Host` → exit 7 mirato |
| Ispezione post-run | metriche (`calls`, `bytes_in`, `bytes_out`) stampate DOPO la run (handler come `&mut dyn`) |
| Politica del budget | il tool-handler ha la PROPRIA soglia locale (`MAX_ASK_CALLS`) sopra quella della sandbox |

## Le tre run di prova incluse nel messaggio d'uso

```sh
cargo run -p scenario-tool-handler -- <guest.wasm> "soma:7x35"       # → tool_result:soma=42
cargo run -p scenario-tool-handler -- <guest.wasm> "reverse:ciao"    # → tool_result:reverse=oaic
cargo run -p scenario-tool-handler -- <guest.wasm> "boom:x"          # → errore host, exit 7
```

## Struttura

```
examples/scenario-tool-handler/
├── README.md          # questo file
├── Cargo.toml         # bin host (wasmbox-core come unica dipendenza)
├── src/main.rs        # ToolHandler + main con gestione SandboxError
└── guest/             # guest wasm32 (cdylib): solo trasporto del protocollo
```
