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
| **HTTP — `wasmbox-http`** | orchestratori remoti (v0.7) | una POST JSON a `/run` | § 4.1 qui sotto |
| **FFI — `wasmbox-ffi`** | host non-Rust (Python/Node/Go…) (v0.7) | ctypes via C-ABI stabile | § 4.2 qui sotto |

Se non sai quale scegliere: **agenti → CLI** (zero Rust lato host);
**applicativo Rust → libreria** (massimo controllo, overhead `ask` ~0.7 µs
misurato nel [benchmark](../crates/wasmbox-core/benches/perf.rs));
**orchestratore remoto → HTTP**; **host non-Rust → FFI**.

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

## 4) Estensioni FATTE in v0.7 (HTTP, FFI, tool LLM)

Le tre estensioni del piano [extension-plan](extension-plan.md) sono
**implementate, testate e prove reali verti**: crate separati sopra
`wasmbox-core`, mai dentro (rispetta i divieti del blueprint).

### 4.1 `wasmbox-http` — orchestrazione remota (scenario D)

Server HTTP std (`TcpListener` + un thread per connessione, zero dipendenze
oltre a `wasmbox-core`; JSON e base64 scritti a mano).

```sh
cargo build -p wasmbox-http --release
wasmbox-http [--bind HOST:PORT]      # default 127.0.0.1:8130 (loopback)
```

Rotta unica `POST /run` + `GET /healthz`:

```json
POST /run {"guest":"<base64>","input":"<base64|null>","limits":{...}}
→ 200 {"ok":true,"output":"<base64>","ask_calls":n}
→ 200 {"ok":false,"error":"fuel_exhausted","exit_code":3}   // errori sandbox, come CLI
→ 400 bad_request · 413 guest_too_large · 404 · 405
```

- `limits` accetta `max_fuel`, `epoch_timeout_ms` (0 = disattivato),
  `max_memory_bytes`, `max_ask_calls` (0 = illimitato),
  `max_ask_payload_bytes` (0 = illimitato): i limiti di risorsa viaggiano
  con la request, senza rebuild.
- Nessun TLS/auth in v0.7: esporre SOLO dietro reverse-proxy con auth;
  bind di default loopback-only; `--bind 0.0.0.0` è un atto deliberato.
- Prova: `sh scripts/scenarios/scenario-d.sh` — 4 case (echo ok, 400,
  invalid_wasm, **fuel deterministico** via `limits.max_fuel`).

### 4.2 `wasmbox-ffi` — C-ABI stabile per host non-Rust (scenario E)

`crates/wasmbox-ffi` (`crate-type = ["cdylib","staticlib"]`, zero
dipendenze oltre al core). L'output di ogni run è copiato in un buffer
proprietario del crate, MAI puntatori alla memoria guest fuori dal crate;
una engine NON è thread-safe; handler `ask` interno = eco (v0.7).

```c
wasmbox_engine_t *wasmbox_engine_new(const uint8_t *wasm, size_t len,
                                     const wasmbox_limits_t *limits); // NULL→default
void wasmbox_engine_free(wasmbox_engine_t *engine);
wasmbox_status_t wasmbox_engine_run(engine, const uint8_t *input, size_t in_len,
                                    uint8_t **out, size_t *out_len);
void wasmbox_buffer_free(uint8_t *buf, size_t len);
```

`wasmbox_status_t`: 0 ok · 1 arg invalido · 2 init fallito · 3 run fallita;
`wasmbox_last_error_code(engine)` restituisce il codice **stile CLI**
(0/3/4/5/6/7/8/9/10); `wasmbox_last_error` → `char*` UTF-8 (copiare subito).

Prova da Python **senza dipendenze** (`ctypes` stdlib):
`sh scripts/scenarios/scenario-e.sh` — 5 case (echo round-trip + errori).

### 4.3 Tool LLM reale (scenario F, feature `llm`)

Sullo scenario B, tool `llm:<prompt>` via OpenRouter (`POST
/api/v1/chat/completions`). La rete sta **interamente nell'host handler**
(feature opt-in `llm` del crate scenario-tool-handler, dipendenza `ureq`
optionale con solo `rustls`): la sandbox e `wasmbox-core` restano senza I/O.

- Richiede `OPENROUTER_API_KEY` nell'env del **processo host** (mai nel
  guest, mai nella risposta). Limiti: prompt ≤ 8 KiB, timeout HTTP 30 s,
  retry 0, modello default `openrouter/auto:free`.
- Senza chiave: `SandboxError::Host` → exit 7. Prova offline:
  `sh scripts/scenarios/scenario-f.sh`.

### Direzioni tradute in extension-plan (storia)

Il piano ha sostituito la vecchia sezione "Non disponibile oggi"; i criteri
restano: **crate ad hoc sopra `wasmbox-core`, mai dentro**.

## 5) Scenari provabili: i tre esempi pronti (examples/)

Tre esempi eseguibili dimostrano le vie 1 e 2 con prove reali; a ciascuno
rimandano anche i link in [index](index.md) e i grafi/preview:

| Scenario | Via | Cartella | Comando | Dimonstra |
|---|---|---|---|---|
| A — pipeline editoriale | 1 (CLI) | [examples/scenario-cli-pipeline](../examples/scenario-cli-pipeline/README.md) | `sh scripts/scenarios/scenario-a.sh` | un agente decide TRUST/REJECT da exit + `--json`, input ostile invertito, registro per SHA-256 |
| B — handler tool | 2 (libreria) | [examples/scenario-tool-handler](../examples/scenario-tool-handler/README.md) | `sh scripts/scenarios/build-scenarios.sh` + `cargo run -p scenario-tool-handler -- <guest.wasm> "soma:7x35"` | `HostHandler` con protocollo `tool:<nome>:<arg>` → `tool_result:…`, metriche post-run, errore host → exit 7 |
| C — guest ostili | 1 (CLI) | [examples/scenario-hostile-guest](../examples/scenario-hostile-guest/README.md) | `sh scripts/scenarios/scenario-c.sh` | fuel/timeout (3 o 4), OOM (5), ask-limit (6): i limiti scattano con l'errore tipizzato, mai crash |
| D — orchestrazione remota | 4 (HTTP) | [`crates/wasmbox-http`](../crates/wasmbox-http/) | `sh scripts/scenarios/scenario-d.sh` | POST /run: echo ok (200), bad_request (400), invalid_wasm, fuel deterministico via `limits` |
| E — FFI reale | 5 (FFI) | [`crates/wasmbox-ffi`](../crates/wasmbox-ffi/) | `sh scripts/scenarios/scenario-e.sh` | ctypes/stdlib: engine new+run+free, status 0/1, wasm invalido → NULL |
| F — tool LLM | 2 (libreria, feature) | [`examples/scenario-tool-handler`](../examples/scenario-tool-handler/) | `sh scripts/scenarios/scenario-f.sh` | senza chiave → exit 7 tipizzato; con OPENROUTER_API_KEY → risposta reale |

Companion dello scenario C: `crates/wat-compile-scenarios` — compila i WAT via
crate `wat` (dev-dep autorizzata), così gli scenari non richiedono wabt esterno.

## 6) Checklist di integrazione (prima di dichiarare finito)

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
- Piano estensioni v0.7: [extension-plan.md](extension-plan.md)
- Contratto CLI per agenti: [SKILL.md](../skills/wasmbox/SKILL.md)
- Esempio host: [`examples/host-run/src/main.rs`](../examples/host-run/src/main.rs)
- Esempio guest: [`examples/guest-echo/src/lib.rs`](../examples/guest-echo/src/lib.rs)
- Benchmark: [`benches/perf.rs`](../crates/wasmbox-core/benches/perf.rs)
- Stato/prossimi passi: [ROADMAP.md](ROADMAP.md)
