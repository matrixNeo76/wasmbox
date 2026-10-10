---
type: Concept
title: "Scenario C — guest ostili: i limiti di risorsa in azione"
description: "Tre guest bomba in WAT (fuel-bomb, oom-bomb, ask-flood) compilati al volo e lanciati via wasmbox-cli --json: verifica che ogni limite scatti con l'errore tipizzato previsto e mai con un crash."
resource: "examples/scenario-hostile-guest/README.md"
tags: ["scenario", "security", "limits", "fuel", "oom", "ask-limit"]
updated: "2026-10-10"
---

# Scenario C — guest ostili: i limiti di risorsa in azione

Dimostra la parte **"limiti"** della guida
([docs/integration.md](../../docs/integration.md), sezione errori e checklist):
tre guest bomba in WAT provano a - perché il runtime li frena con l'errore
**tipizzato** previsto e il processo host non crasha mai.

## Le tre bombe

| Guest | Attacco | Limite che scatta | Exit atteso |
|---|---|---|---|
| `fuel-bomb.wat` | loop infinito | `max_fuel` oppure epoch 1 s (il primo che vince: **non deterministico**, entrambi limiti validi) | 3 oppure 4 |
| `oom-bomb.wat` | input 20 MiB via stdin, guest_alloc → 0 | `ReadLimits` → `guest_out_of_memory` | 5 |
| `ask-flood.wat` | 100 000 chiamate `ask` in cascata | `max_ask_calls` (1024) | 6 |

## Come si esegue

```sh
sh scripts/scenarios/scenario-c.sh    # compila le bombe e le alleva una per volta
```

Lo script verifica per ciascuna: exit **esattamente** quello previsto,
`ok:false` con l'`error` giusto nel JSON, e **nessun crash** della shell
(diagnostic finale tutti-PASS o FAIL). Utle per regressioni: se un limite
smette di scattare, lo script fallisce.

## Come si estende

Un guest bomba è un file WAT con la stessa ABI (vedi template nel runner):
aggiungi la tua `*.wat` e la riga corrispondente nella tabella del runner.
La suite e2e del core
([e2e_test.rs](../../crates/wasmbox-core/tests/e2e_test.rs)) copre varianti
più fini (boundary esatti del limite, `memory.grow` rifiutato con -1).

## Struttura

```
examples/scenario-hostile-guest/
├── README.md        # questo file
├── fuel-bomb.wat    # loop infinito
├── oom-bomb.wat     # allocazione crescente
└── ask-flood.wat    # flood di richieste ask
```

Il runner vive in `scripts/scenarios/scenario-c.sh`.
