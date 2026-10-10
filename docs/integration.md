---
type: Guide
title: "wasmbox — guida all'integrazione (sistemi, applicativi, agenti)"
description: "Come integrare wasmbox: via binaria (CLI, per agenti AI), via libreria (Rust, per applicativi), authoring di guest conformi, protocollo su ask, errori, limiti, checklist di validazione."
resource: "docs/integration.md"
tags: ["integration", "guide", "host-handler", "agents", "cli", "ask-protocol"]
updated: "2026-10-10"
---

# GUIDA ALL'INTEGRAZIONE — wasmbox in altri sistemi, applicativi e agenti

> Pubblico: chi vuole *usare* wasmbox dentro un proprio sistema mantenendo i
> limiti di risorsa. Tre strade possibili, una sola regola architetturale:
> **la logica di dominio sta nell'host, mai nella sandbox**.

## Le tre vie d'uso (quale scegliere)

| Via | Per chi | Cosa scrivi tu | Contratto |
|---|---|---|---|
| **Binaria — `wasmbox-cli`** | Agenti AI, script, pipeline | il guest (e nient'altro) | [SKILL.md](../skills/wasmbox/SKILL.md): exit 0–10 deterministici, `--json` |
| **Libreria — `wasmbox-core`** | Applicativi Rust con dominio reale | un `HostHandler` + gestione `SandboxError` | questo documento |
| **FFI / endpoint HTTP** | host non-Rust, orchestratori remoti | un crate ad hoc | **non avviati** (deliberato, vedi [ROADMAP §2.9](ROADMAP.md)) — sezione "Estensioni" sotto |

Se non sai quale scegliere: **agenti → CLI** (zero Rust lato host);
**applicativo Rust → libreria** (massimo controllo, overhead `ask` ~0.7 µs
misurato nel [benchmark](../crates/wasmbox-core/benches/perf.rs)).

## 1) Via binaria: la CLI per agenti AI

Firma:

```sh
wasmbox-cli run <guest.wasm> [input] [--json]
```

`--json` emette **una sola riga** su stdout:
`{"ok":true,"output":"…"}` oppure `{"ok":false,"error":"snake_case","message":"…"}`.

Exit code — causo e significato operativo:

| Exit | JSON `error` | Causa | Azione consigliata |
|---|---|---|---|
| 0 | — | run ok | — |
| 2 | — | uso/IO errato (file assente ecc.) | correggere il comando, non il guest |
| 3 | `fuel_exhausted` | budget di istruzioni esaurito | **non ritentare identico**: ridurre/rivedere il guest |
| 4 | `timeout` | wall-clock superato | idem |
| 5 | `guest_out_of_memory` | troppa memoria/frammentazione | idem |
| 6 | `ask_limit_exceeded` / `payload_too_large` | abuso del canale host | ridurre payload e frequenza delle `ask` |
| 7 | — | errore dell'host handler (lato host) | correggere l'host, non il guest |
| 8 | — | export mancante nel guest | guest non conforme all'[ABI](blueprint.md) |
| 9 | — | `.wasm` non valido | rigenerare |
| 10 | — | esecuzione generica | ispezionare `message` |

Direttive dure (riprese da [SKILL.md](../skills/wasmbox/SKILL.md)):

- con consumatore macchina: **sempre** `--json`;
- **mai ritentare identico** su `fuel_exhausted`/`timeout`/`guest_out_of_memory`:
  indicano guest malformato o sovradimensionato;
- il guest è **non fidato**: nessun accesso a filesystem/rete/processi.
  Nella CLI `ask` fa solo "eco-maiuscolo": è una demo di test — per logica
  di dominio vera si usa la via libreria (§ 2).

### Ciclo tipico per un agente che genera guest

1. **Genera** il codice guest (Rust `cdylib` wasm32 conforme all'ABI — § 3).
2. **Compila**: `cargo build --target wasm32-unknown-unknown --release`.
3. **Esegui**: `wasmbox-cli run guest.wasm "<input>" --json`.
4. **Interpreta**: `ok:true` → output; errore di risorsa (3/4/5/6) → ridimensionare
   e rigenerare un guest diverso; errore di forma (8/9) → correggere l'ABI.
5. **Archivia** i guest validi per hash del contenuto: la sandbox **cachetta**
   le compilazioni in `cache_dir` automaticamente (compile freddo 1.355 ms →
   cache-hit 0.136 ms).

## 2) Via libreria: `wasmbox-core` in un applicativo Rust

Dipendenza (dal percorso del repo, o da crates.io quando pubblicato):

```toml
[dependencies]
wasmbox-core = { path = "../wasmbox/crates/wasmbox-core" }
```

### Anatomia minimale

```rust
use wasmbox_core::{HostError, HostHandler, SandboxConfig, SandboxEngine};

struct MyHandler { /* stato di dominio */ }

impl HostHandler for MyHandler {
    fn ask(&mut self, request: &[u8]) -> Result<Vec<u8>, HostError> {
        // Unica porta tra guest e mondo: qui sta la TUA logica
        // (una ricerca, un tool, un motore di regole, una chiamata API…).
        Ok(self.rispondi(request)?)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = std::fs::read("guest.wasm")?;
    let config = SandboxConfig {
        cache_dir: Some(std::env::temp_dir().join("mio-cache")),
        ..Default::default()     // default già sicuri (tabella sotto)
    };
    let engine = SandboxEngine::new(&wasm, config)?;   // 1 engine per guest
    let output = engine.run(b"input", &mut MyHandler { /* … */ })?;
    Ok(())
}
```

Esempio completo già nel repo e testato:
[`examples/host-run/src/main.rs`](../examples/host-run/src/main.rs).

Regole d'integrazione non negoziabili:

- **Un `SandboxEngine` per guest, riusabile serialmente**: lo costruisci con
  `new()` e lo usi per tutte le run dello stesso modulo. **Niente un thread per
  `run()`**: la run resta sul thread chiamante (competenza della tua app
  parallelizzare a livello di processi o pool esterni).
- L'handler passa come **`&mut dyn HostHandler`** (non `Box`): dopo la `run()`
  puoi ispezionarne lo stato (contatori, cache di sessione, metriche…).
- **Gestisci sempre l'`Err`**: ogni errore della sandbox è tipizzato
  (`SandboxError`). In produzione mai `.unwrap()` sulla run.

### `SandboxConfig` campo per campo

| Campo | Default | Uso | Consiglio |
|---|---|---|---|
| `max_fuel: Option<u64>` | `Some(1_000_000_000)` | tetto istruzioni (`None` = disabilitato) | lascialo attivo per guest non fidati |
| `epoch_timeout: Option<Duration>` | `Some(1000 ms)` | timeout wall-clock (ticker da 10 ms) | alza (2–10 s) se il guest è legittimamente lungo |
| `max_memory_bytes` | `16 MiB` | tetto memoria lineare del guest | aumenta solo con misurazioni reali |
| `pool_size` | `8` | dimensione pooling allocator | non toccare se non sai di averne bisogno |
| `cache_dir: Option<PathBuf>` | `None` | cache `.cwasm` precompilata | imposta una dir dedicata: 1.355 ms → 0.136 ms |
| `max_ask_calls` | `1024` | budget di chiamate `ask` per run | riduci se il tuo protocollo usa poche round-trip |
| `max_ask_payload_bytes` | `1 MiB` | tetto per richiesta E risposta | adatta al tuo formato reale |

Perf misurato (linux x86_64, release — [perf.rs](../crates/wasmbox-core/benches/perf.rs)):
compile freddo **1.355 ms**, cache-hit **0.136 ms**, overhead round-trip `ask`
**~0.7 µs**.

### Errori: mappa completa `SandboxError` → politica

Fonte unica: [`error.rs`](../crates/wasmbox-core/src/error.rs).

| Variant | Causa | Politica dell'host |
|---|---|---|
| `EngineInit(String)` | init Wasmtime/linker fallito | config errata: fallisci fast e logga |
| `InvalidWasm(String)` | modulo non valido | rifiuta il guest |
| `MissingExport(String)` | manca `memory`/`guest_alloc`/`guest_free`/`guest_run` | guest non conforme ABI → rifiuta |
| `FuelExhausted` | budget istruzioni finito | rifiuta: guest troppo grezzo, rivederlo |
| `Timeout` | wall-clock scaduto (epoch) | rifiuta; non alzare il timeout come rimedio al guest |
| `GuestOutOfMemory` | OOM del guest | rifiuta; rivede il guest |
| `AskLimitExceeded(n)` | superato `max_ask_calls` | possibile abuso o design sbagliato: rifiuta |
| `PayloadTooLarge{size,max}` | tetto payload superato | restringi il payload; alza il limite solo verificato |
| `Host(HostError)` | errore del TUO handler | è il tuo errore: gestiscilo dove hai deciso |
| `Execution(String)` | trap generico | logga `message`, ispeziona; consideralo bug del guest |

I limiti di risorsa non sono fallimenti operativi: quando scattano indicano
guest che va **corretto**, non ritentato identico.

### Protocollo lato host sopra `ask`

`ask` scambia **bytes opachi** e la sandbox non li interpreta — così il dominio
resta nell'host (vincolo del blueprint). Il modo più semplice per renderli usabili:

1. definisci una **struct Request/Response** nel tuo crate host
   (JSON, MessagePack, bincode… qualsiasi formato: la sandbox è agnostica);
2. `HostHandler::ask` decodifica la Richiesta, fa il lavoro (tool, DB, API…),
   codifica la Risposta e la restituisce;
3. il guest fa la parte speculare: componi la Richiesta in memoria,
   chiama `ask`, decodifica la Risposta → output,
   e restituisce il puntatore packed `(ptr << 32) | len`.

Anti-pattern da evitare:

- non copiare logica di dominio nel guest (viola il blueprint: il core e il
  guest devono restare senza dominio);
- non usare `ask` per cose note a tempo di compilazione: il protocollo serve
  per interazioni eseguite in runtime.

## 3) Authoring di un guest conforme (ABI)

Fonte: sezione ABI del [blueprint](blueprint.md). Un guest deve:

| Export | Firma | Nota |
|---|---|---|
| `memory` | `(memory 1)` | memoria lineare condivisa |
| `guest_alloc` | `(i32) -> i32` | allocazione; `len == 0` → dangling non-null |
| `guest_free` | `(i32, i32)` | deallocazione, no-op su `len == 0`/null |
| `guest_run` | `(i32, i32) -> i64` | entry point: input → output packed |

E può importare **solo**:

| Import | Firma | Nota |
|---|---|---|
| `env::ask` | `(i32, i32) -> i64` | l'unica capability: richiesta→risposta packed |

Parti da [`examples/guest-echo/src/lib.rs`](../examples/guest-echo/src/lib.rs)
(completo, già compilato e testato). Schema:

```rust
// lib.rs — cdylib per wasm32-unknown-unknown
use std::alloc::{alloc, dealloc, Layout};

#[link(wasm_import_module = "env")]
extern "C" { fn ask(req_ptr: i32, req_len: i32) -> i64; }

#[no_mangle] pub extern "C" fn guest_alloc(len: u32) -> *mut u8 { /* Layout alloc */ }
#[no_mangle] pub unsafe extern "C" fn guest_free(ptr: *mut u8, len: u32) { /* dealloc */ }
#[no_mangle] pub unsafe extern "C" fn guest_run(in_ptr: i32, in_len: i32) -> i64 {
    // 1. leggi input da (in_ptr..in_ptr+in_len)
    // 2. componi richiesta, chiama ask(req_ptr, req_len) → i64 packed (ptr,len)
    // 3. decodifica risposta, componi output, restituisci (out_ptr << 32) | out_len
}
```

Validazione di un tuo guest in autonomia:

1. `cargo build --target wasm32-unknown-unknown --release` — exit 0;
2. `wasmbox-cli run guest.wasm "<input>" --json` — `ok:true` e output atteso;
3. **input ostile**: stringa vuota, stringa ×1 MB, byte non-UTF8 —
   atteso: errore tipizzato (exit 5/6/10), **mai** crash del processo
   `wasmbox-cli`.

## 4) Estensioni (HTTP, FFI, host non-Rust)

**Non disponibile oggi, deliberatamente** — [ROADMAP §2.9](ROADMAP.md). Se ti
serve, le direzioni:

- **host non-Rust, oggi**: usa la CLI come subprocess: JSON + exit
  deterministici è un contratto già pensato per agenti in qualunque linguaggio;
- **endpoint HTTP** per orchestratori remoti: richiede una tua decisione
  esplicita; sarà un crate separato (il core resta libreria pura);
- **FFI/C-ABI** per altre lingue nativamente: stesso criterio — crate ad hoc
  sopra `wasmbox-core`, mai dentro.

## 5) Checklist di integrazione (prima di dichiarare finito)

- [ ] Guest conforme all'ABI: `ok:true` con input normale, errore tipizzato
      `ok:false` con input malformato (mai crash del processo).
- [ ] `SandboxConfig` coerente col carico: al più una prova in cui un guest
      ostile scatena fuel/timeout/OOM/ask-limit e viene **rifiutato**
      pulitamente (nessun crash dell'host, nessun side-effect).
- [ ] Handler: nessun `unwrap()` lato host; errore handler verificato
      (exit 7 via CLI, `SandboxError::Host` da libreria).
- [ ] Cache attiva su `cache_dir` dedicato; nessuna condivisione di `.cwasm`
      tra OS/architetture diverse (non portabile).
- [ ] Se l'input viene da un LLM/utente: il guest è generato/compilato in una
      fase separata ed eseguito **solo** dentro wasmbox.
- [ ] La logica di dominio vive in un TUO crate applicativo, non in
      `wasmbox-core` né nel guest (vincolo del blueprint).

## Riferimenti

- Spec vincolante: [blueprint.md](blueprint.md)
- Contratto CLI per agenti: [SKILL.md](../skills/wasmbox/SKILL.md)
- Esempio host: [`examples/host-run/src/main.rs`](../examples/host-run/src/main.rs)
- Esempio guest: [`examples/guest-echo/src/lib.rs`](../examples/guest-echo/src/lib.rs)
- Benchmark: [`benches/perf.rs`](../crates/wasmbox-core/benches/perf.rs)
- Stato/prossimi passi: [ROADMAP.md](ROADMAP.md)
