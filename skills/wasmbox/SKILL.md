---
type: Skill
title: "Skill agente AI — usare wasmbox-cli"
description: "Come un agente AI (o uno script) usa la sandbox wasmbox tramite la CLI: invocazione, exit code, JSON, limiti."
resource: "skills/wasmbox/SKILL.md"
tags: ["agent", "cli", "json", "exit-codes", "wasmbox"]
updated: "2026-10-09"
---

# Skill: eseguire guest WebAssembly in sandbox con `wasmbox-cli`

## Obiettivo

Eseguire un guest `.wasm` **non fidato** in una sandbox con limiti di risorsa
(fuel, memoria, timeout, budget di chiamate host) e ricevere l'output o un
errore tipizzato, senza mai compromettere l'host.

## Prerequisiti

- Binario `wasmbox-cli` compilato: `cargo build -p wasmbox-cli`.
  Percorso: `target/debug/wasmbox-cli`.
- Un guest `.wasm` che esporta `memory`, `guest_alloc`, `guest_free`,
  `guest_run` e (opzionale) importa `env::ask`. In questo repo c'è
  `guest-echo`: `target/wasm32-unknown-unknown/release/guest_echo.wasm`
  (compila con `sh ./scripts/build.sh`).

## Invocazione

```sh
# 1) Output umano (stdout = output del guest, log su stderr)
target/debug/wasmbox-cli run <guest.wasm> "input del guest"

# 2) Input da stdin (utile in pipe / agenti)
cat input.txt | target/debug/wasmbox-cli run <guest.wasm>

# 3) Output JSON machine-readable (una riga, exit code sempre 0/2/3/…10)
target/debug/wasmbox-cli run <guest.wasm> "input" --json
```

## Contratto di uscita (per agenti)

| Exit | Significato |
|---|---|
| 0 | `ok:true` / output del guest su stdout |
| 2 | uso errato o I/O locale (file .wasm non leggibile, comando sconosciuto) |
| 3 | `FuelExhausted` — il guest ha superato il budget CPU |
| 4 | `Timeout` — superato il limite wall-clock (default 1000 ms) |
| 5 | `GuestOutOfMemory` — il guest ha superato `max_memory_bytes` (16 MiB) |
| 6 | `AskLimitExceeded` o `PayloadTooLarge` — abuso del canale host |
| 7 | errore dell'`HostHandler` (lato host, deciso dalla shell che invoca) |
| 8 | `MissingExport` — guest privo di `memory`/`guest_alloc`/`guest_free`/`guest_run` |
| 9 | `.wasm` non valido |
| 10 | altro errore di esecuzione o inizializzazione |

Con `--json` **una singola riga JSON** su stdout:
`{"ok":true,"output":"…"}` oppure `{"ok":false,"error":"fuel_exhausted","message":"…"}`.
Il campo `error` usa snake_case; il mapping è quello della tabella qui sopra.

## Politica di default consigliata per agenti

- **Eseguire sempre con `--json`** per parsing affidabile; riservare l'output
  umano all'utente.
- **Mai superare 10s di attesa** (il timeout interno è 1000 ms di default);
  se serve attesa più lunga, cambiare `epoch_timeout` nel codice host, non in
  CLI (la CLI non espone tuning, deliberatamente: minima superficie).
- Se `error` è `fuel_exhausted`/`timeout`/`guest_out_of_memory`, il guest si è
  comportato male: **non ritentare con lo stesso guest**, rivederlo.
- Trattare il guest come **non fidato**: la CLI non espone filesystem/rete;
  tutto ciò che il guest può fare passa per `ask`, e qui fa solo eco.

## Limiti espliciti della skill

- La CLI fa solo "eco/maiuscolo" come handler: è una **demo di test**, non un
  servizio. Un agente che ha bisogno di logica di dominio reale (tool, API,
  RAG …) deve implementare un proprio `HostHandler` in Rust (vedi
  `examples/host-run`) e usare `wasmbox-core` come libreria.
- Per estensioni (CLI con protocollo, endpoint HTTP, FFI) vedi
  `docs/ROADMAP.md` §2.9.
