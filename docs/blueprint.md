---
type: Specification
title: "BLUEPRINT wasmbox — specifica di riferimento v0.5"
description: "Spec autorevole e vincolante del prodotto: obiettivo, struttura workspace, dipendenze, ABI, file per file, divieti, ordine di implementazione, test, validazione."
resource: "docs/blueprint.md"
tags: ["spec", "blueprint", "abi", "wasmtime", "sandbox"]
updated: "2026-10-09"
---

# BLUEPRINT — `wasmbox` (specifica di riferimento v0.5)

> Questo documento è la specifica autorevole del progetto. Va salvato prima di scrivere codice
> (richiesta esplicita dell'utente) e consultato durante tutta l'implementazione.
>
> v0.3 (2026-10-08): migrazione a **wasmtime 49.0** — dipendenza `anyhow` rimossa;
> host function e `map_guest_error` usano `wasmtime::Result` / `wasmtime::Error`.
>
> v0.4 (2026-10-09): aggiunte post-validazione approvate dall'utente —
> **`examples/host-run`** (membro workspace, esempio lato host), **`benches/perf.rs`**
> (`harness = false`, solo std: nessuna dipendenza aggiunta), **7 test di robustezza**
> (boundary payload, memoria, concorrenza → 27 totali) e **metadati crates.io**
> (`repository`/`homepage`/`readme`/`keywords`/`categories`, `cargo publish --dry-run` OK).
>
> v0.5 (2026-10-09): **interfacce di fruizione** — la libreria rimaneva
> inutilizzabile senza scrivere Rust. Aggiunti: **`crates/wasmbox-cli`** (CLI per umani e agenti AI:
> input da argv/stdin, `--json` machine-readable, exit code deterministici),
> **`crates/wasmbox-ui`** (UI minimale Slint con screenshot BMP headless via
> `SoftwareRenderer` su `MinimalSoftwareWindow`, senza fontconfig di sistema:
> font embedded), **`skills/wasmbox/SKILL.md`** (come un agente usa la CLI:
> invocazione, tabella exit code, politica di default). Regola: la logica di dominio
> resta FUORI da wasmbox-core — CLI e UI sono applicazioni separate che usano
> solo l'API pubblica (`SandboxEngine`, `HostHandler`, `SandboxConfig`,
> `SandboxError`); nessun cambio all'ABI né ai divieti.`

# OBIETTIVO

Costruire `wasmbox`, un workspace Rust che è il *contenitore* per eseguire codice WebAssembly non fidato (plugin, script LLM, agenti) con:
- nessun accesso a filesystem/rete/risorse di sistema da parte del guest;
- limiti espliciti su CPU (fuel), memoria (`StoreLimits`) e tempo wall-clock (epoch interruption);
- una sola funzione di comunicazione guest→host: `env::ask(req_ptr, req_len) -> i64`, con payload bytes **opachi** (il runtime non ne interpreta il contenuto — la logica di dominio vive interamente nell'handler dell'host).

PRINCIPIO ARCHITETTURALE: `wasmbox-core` non sa nulla del mondo esterno. Sa solo eseguire Wasm con limiti di risorsa e inoltrare richieste opache a un `HostHandler` fornito dall'host.

# STRUTTURA DEL WORKSPACE (esatta)

```
wasmbox/
├── Cargo.toml                    # workspace root, members = ["crates/wasmbox-core", "examples/guest-echo", "examples/host-run"]
├── docs/blueprint.md             # questo documento
├── crates/wasmbox-core/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs                # 4 pub mod (config, error, memory, engine) + 3 pub use (SandboxConfig, SandboxError/HostError, SandboxEngine/HostHandler)
│   │   ├── config.rs
│   │   ├── error.rs
│   │   ├── memory.rs
│   │   └── engine.rs
│   ├── benches/perf.rs           # v0.4: bench harness=false, solo std
│   └── tests/e2e_test.rs
├── examples/guest-echo/
│   ├── Cargo.toml                # crate-type = ["cdylib"]
│   └── src/lib.rs
└── examples/host-run/            # v0.4: esempio lato host
    ├── Cargo.toml                # bin, dipende da wasmbox-core (path)
    └── src/main.rs
```

Interfacce (v0.5):
```
├── crates/wasmbox-cli/           # v0.5: CLI per umani + agenti AI
│   └── src/main.rs               # run <guest.wasm> [input] [--json]; exit 0/2..10
├── crates/wasmbox-ui/            # v0.5: UI minimale Slint headless
│   ├── Cargo.toml                # slint default-features = false (no fontconfig)
│   ├── build.rs                  # compile + embed_resources (font)
│   ├── sandbox.slint             # componente sandboxwindow, senza testo
│   └── src/main.rs               # --screenshot out.bmp → BMP 24-bit puro Rust
└── skills/wasmbox/SKILL.md       # v0.5: skill agente per usare la CLI
```

# DIPENDENZE

`crates/wasmbox-core/Cargo.toml`:
```toml
[dependencies]
wasmtime = { version = "49.0", default-features = false, features = ["std", "runtime", "cranelift", "pooling-allocator", "cache", "parallel-compilation"] }
thiserror = "2.0"

[dev-dependencies]
wat = "1"

[[bench]]
name = "perf"
harness = false   # v0.4: bench solo-std, nessuna dipendenza aggiunta
```
NIENTE wasmtime-wasi, niente serde/serde_json, niente `anyhow` (rimosso con la migrazione a 49:
le host function restituiscono `wasmtime::Result`, che è l'unico tipo accettato da `IntoFunc`),
niente altre dipendenze. Il bench v0.4 è scritto con la sola `std` + la dev-dep `wat`
per rispettare questo divieto (niente criterion).

# CONTRATTO ABI (vincolante)

Guest deve esportare: `memory` (memory 1), `guest_alloc(i32)->i32`, `guest_free(i32,i32)`, `guest_run(i32,i32)->i64`.
Guest può importare solo: `env::ask(i32,i32)->i64`.

Regole critiche:
- `guest_alloc(0)` deve restituire un puntatore non-nullo (dangling ok); `guest_free(ptr, 0)` è no-op.
- `guest_alloc(len>0) == 0` ⇒ out of memory: l'host deve fallire con `GuestOutOfMemory`, MAI scrivere all'offset 0.
- Packing: `pack_ptr_len(ptr,len) -> u64 = ((ptr as u64) << 32) | len as u64`; `unpack_ptr_len(u64) -> (u32,u32)`. `ask` restituisce `pack(...) as i64`.

# FILE PER FILE

## src/error.rs
```rust
use thiserror::Error;

#[derive(Debug, Clone, Error)]   // Clone necessario: map_guest_error fa h.clone()
pub enum HostError {
    #[error("errore generico dell'host: {0}")]
    Custom(String),
    #[error("budget di chiamate host esaurito")]
    BudgetExceeded,
}

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("inizializzazione Wasm fallita: {0}")] EngineInit(String),
    #[error("compilazione o modulo non valido: {0}")] InvalidWasm(String),
    #[error("accesso alla memoria Wasm fallito: {0}")] MemoryAccess(String),
    #[error("esportazione richiesta assente nel guest: {0}")] MissingExport(String),
    #[error("fuel esaurito: limite di computazione superato")] FuelExhausted,
    #[error("timeout wall-clock superato (epoch interrupt)")] Timeout,
    #[error("allocazione guest fallita: memoria insufficiente (out of memory)")] GuestOutOfMemory,
    #[error("superato il limite massimo di chiamate 'ask' ({0})")] AskLimitExceeded(u32),
    #[error("payload 'ask' oltre il limite: {size} byte (max {max})")] PayloadTooLarge { size: usize, max: usize },
    #[error("errore restituito dall'host handler: {0}")] Host(#[from] HostError),
    #[error("guest trap / errore di esecuzione: {0}")] Execution(String),
}
```

## src/config.rs
`pub struct SandboxConfig` con `Clone` e `Default`. Campi:
- `max_fuel: Option<u64>` — default `Some(1_000_000_000)`; se `Some`, `Config::consume_fuel(true)`.
- `epoch_timeout: Option<Duration>` — default `Some(Duration::from_millis(1000))`; se `Some`, `epoch_interruption(true)`.
- `max_memory_bytes: usize` — default `16 * 1024 * 1024`.
- `pool_size: u32` — default `8` (total_memories / total_stacks / total_core_instances).
- `cache_dir: Option<PathBuf>` — default `None`.
- `max_ask_calls: u32` — default `1024`.
- `max_ask_payload_bytes: usize` — default `1024 * 1024`.
- `pub(crate) const EPOCH_TICK: Duration = Duration::from_millis(10);` — usato sia come sleep del ticker sia come granularità del deadline (`ticks = (timeout.as_millis() / EPOCH_TICK.as_millis()).max(1)`).

## src/memory.rs
Funzioni pure `pack_ptr_len(ptr: u32, len: u32) -> u64` e `unpack_ptr_len(packed: u64) -> (u32, u32)` + unit test round-trip (0,0), (1, u32::MAX), (u32::MAX,1), valori casuali.

## src/engine.rs (il cuore)
Ordine di implementazione: `compute_wasm_hash` → `StoreContext` → `ask_host_function` → `SandboxEngine::new` → `SandboxEngine::run` → `map_guest_error` → `Drop`.

- `compute_wasm_hash(&[u8]) -> String` con `std::collections::hash_map::DefaultHasher`, `format!("{:016x}", hasher.finish())` — hash deterministico non crittografico, usato come cache key.
- `struct StoreContext { config: SandboxConfig, calls_made: u32, handler: *mut (dyn HostHandler + 'static), limits: StoreLimits }` con `unsafe impl Send` (usato solo nello scope di `run()`, mai condiviso tra thread).
- `pub trait HostHandler { fn ask(&mut self, request: &[u8]) -> Result<Vec<u8>, HostError>; }`
- `pub struct SandboxEngine { engine: Engine, instance_pre: InstancePre<StoreContext>, config: SandboxConfig, ticker_stop: Arc<AtomicBool>, ticker_handle: Option<JoinHandle<()>> }`

`new(wasm, config)`:
1. `Config::new()`, `cranelift_opt_level(OptLevel::Speed)`, conditional `consume_fuel` / `epoch_interruption`.
2. Pooling allocator: `PoolingAllocationConfig` con `max_memory_size(config.max_memory_bytes)` (BYTE, non pagine), `total_memories/total_stacks/total_core_instances = config.pool_size`; `allocation_strategy(InstanceAllocationStrategy::Pooling(...))`.
3. Modulo: se `cache_dir` è `Some` — `fs::create_dir_all`, hash del bytecode, path `{hash}.cwasm`. Se esiste: `unsafe { Module::deserialize_file(&engine, path) }`; su Err (cache corrotta/incompatibile) `remove_file` e ricompila. Altrimenti `compile_and_cache()`: `Module::new` + `module.serialize()` scritto su tmp `{hash}.{pid}.tmp` e `fs::rename` atomico; il fallimento della scrittura in cache NON è fatale. Se `cache_dir` è `None`: `Module::new` diretto.
4. `Linker<StoreContext>` con `func_wrap("env", "ask", ask_host_function)`, poi `linker.instantiate_pre(&module)` → `InstancePre` (costruita UNA volta, fuori da `run()`).
5. Se `epoch_timeout` è `Some`: spawn di UN SOLO thread `"wasmbox-epoch-ticker"` che fa `sleep(EPOCH_TICK); engine.increment_epoch()` finché `ticker_stop` non è true. Mai un thread per run.

`ask_host_function(mut caller: Caller<'_, StoreContext>, req_ptr: i32, req_len: i32) -> wasmtime::Result<i64>` (MAI `Trap::new`, così gli errori tipizzati sopravvivono al downcast; gli `SandboxError` si convertono con `.into()` via `From<E: Error>` e restano downcastabili):
1. controllo `calls_made >= max_ask_calls` ⇒ `AskLimitExceeded`;
2. controllo `req_len > max_ask_payload_bytes` ⇒ `PayloadTooLarge`;
3. lettura richiesta da `memory.data(&caller)` con bounds check `ptr.checked_add(len)` ⇒ `MemoryAccess` fuori bounds; `calls_made += 1`;
4. `unsafe { (*caller.data_mut().handler).ask(&req_bytes) }` ⇒ `SandboxError::Host(e)` (deref del puntatore: secondo blocco `unsafe`);
5. controllo payload risposta (stesso max);
6. chiama l'export `guest_alloc` del guest (`typed::<u32,u32>`); se `resp_len > 0 && resp_ptr == 0` ⇒ `GuestOutOfMemory`;
7. scrittura risposta con bounds check; `Ok(pack_ptr_len(resp_ptr, resp_len) as i64)`.

`run(&self, input: &[u8], handler: &mut dyn HostHandler) -> Result<Vec<u8>, SandboxError>`:
- `StoreLimitsBuilder::new().memory_size(max_memory_bytes).instances(1).build()`.
- PRIMO blocco `unsafe`: `let handler_ptr: *mut (dyn HostHandler + 'static) = unsafe { std::mem::transmute(handler as *mut dyn HostHandler) };` — il cast `as` NON compila (lifetimes diversi sul fat pointer); documentare l'invariante: lo Store non sopravvive allo scope di `run()`.
- `Store::new`, `store.limiter(...)`, `set_fuel` se Some, `set_epoch_deadline(ticks)` se Some.
- `instance_pre.instantiate(&mut store)`; recupera export `memory`, `guest_alloc`, `guest_free`, `guest_run` (mancanza ⇒ `MissingExport`).
- Input: se vuoto `(0,0)`, altrimenti `guest_alloc(len)` con `ptr==0` ⇒ `GuestOutOfMemory`, bounds check, `copy_from_slice`.
- `guest_run.call(...)` con errore mappato via `map_guest_error`.
- Output: `unpack_ptr_len`, bounds check, `to_vec()`.
- `guest_free` su input e output se `len > 0` (errore ignorato).
- L'handler resta accessibile al chiamante dopo la run (API prende `&mut dyn HostHandler`, non `Box<dyn>`).

`map_guest_error(e: &wasmtime::Error) -> SandboxError`: iterare `e.chain()` (NON `downcast_ref` sul solo livello esterno): per ogni causa, prima `downcast_ref::<SandboxError>()` (ricostruire il variant owned, `Host(h.clone())`), poi `downcast_ref::<wasmtime::Trap>()`: `OutOfFuel ⇒ FuelExhausted`, `Interrupt ⇒ Timeout`, `UnreachableCodeReached ⇒ GuestOutOfMemory`, altro ⇒ `Execution(trap.to_string())`. Fallback: `Execution(e.to_string())`.

`Drop`: `ticker_stop.store(true)` + `join()` del ticker.

## examples/guest-echo/src/lib.rs
Guest Rust compilato per `wasm32-unknown-unknown`, `crate-type = ["cdylib"]`, con `extern "C" { fn ask(req_ptr: i32, req_len: i32) -> i64; }`:
- `guest_alloc(len)`: `len==0` ⇒ dangling non-null; `Layout::from_size_align(len, 8)`; `std::alloc::alloc`; null su errore layout.
- `guest_free(ptr, len)`: no-op se `len==0 || ptr.is_null()`; altrimenti `dealloc` con lo stesso layout.
- `guest_run(in_ptr, in_len)`: costruisce richiesta `b"inspect:" + input`, chiama `ask`, legge la risposta dal packed, la libera con `guest_free(resp_ptr, resp_len)` (fix leak), produce output `b"echo_result:" + host_resp` allocato con `guest_alloc`, restituisce `(ptr << 32) | len`.

# VIETATI (controlla dopo ogni file)

1. Nessun `wasmtime-wasi` — clock/random, se servono, si espongono come `ask` tipizzato.
2. Nessun `std::process::Command`.
3. Nessuna logica di dominio in `wasmbox-core`: niente `serde_json`, niente termini di dominio (backup, scan, robocopy, restore, sync) come identificatori; `std::fs` ammesso SOLO per la cache `.cwasm`.
4. Nessun `Box<dyn HostHandler>` in `run()`.
5. Mai ricompilare il modulo dentro `run()` — solo `SandboxEngine::new`.
6. Mai un thread per `run()` — ticker unico per engine.
7. Mai appiattire errori in `Trap::new` dentro l'host function.
8. `map_guest_error` solo con `e.chain()`, mai `downcast_ref` sul livello esterno.
9. API Wasmtime 49.0: le host function usano `wasmtime::Result<T>` (mai `anyhow::Result` — `IntoFunc` non lo accetta più); `PoolingAllocationConfig::max_memory_size` in byte; `Module::serialize` / `Module::deserialize_file` per la cache. In caso di dubbio su una firma, consultare docs.rs per wasmtime 49.0.
10. Solo due blocchi `unsafe` in `engine.rs` (transmute fat pointer + deref handler), entrambi documentati con invariante SAFETY. Se un cast `as` non compila tra `&mut dyn Trait` e `*mut (dyn Trait + 'static)`, la soluzione è il transmute confinato, non altre acrobazie.

# ORDINE DI IMPLEMENTAZIONE

1. Struttura directory + `Cargo.toml` workspace → `cargo check --workspace`.
2. `error.rs` → `config.rs` → compila.
3. `memory.rs` + unit test round-trip → compila.
4. `engine.rs` nell'ordine sopra → `cargo check -p wasmbox-core`.
5. `examples/guest-echo` → `cargo build -p guest-echo --target wasm32-unknown-unknown --release`.
6. `tests/e2e_test.rs` → `cargo test -p wasmbox-core -- --nocapture`. Nessun test fallito si passa oltre.
7. Validazione finale (vedi sotto).

# TEST (tests/e2e_test.rs)

Fixture `ECHO_WAT`: modulo WAT con import `env::ask`, export `memory`, `guest_alloc` (bump allocator fittizio: base 1024, avanzamento allineato a 8, `len==0` ⇒ restituire 1024, ritorno `0` solo se finito lo spazio), `guest_free` no-op, `guest_run` che inoltra i byte di input a `ask` e restituisce la risposta dell'host come output (packed).

`EchoHandler` implementa `HostHandler`: risponde `format!("host_saw[{}]", String::from_utf8_lossy(req)).into_bytes()`.

Test richiesti (tutti devono passare):
1. `pack/unpack round-trip` (unit test in memory.rs).
2. Echo round-trip: `engine.run(b"a", &mut EchoHandler)` ⇒ `b"host_saw[a]"`.
3. Input vuoto: `run(b"", ...)` funziona senza allocare input.
4. `FuelExhausted`: WAT con loop infinito, `max_fuel` piccolo, `epoch_timeout: None`.
5. `Timeout`: stesso loop, `max_fuel: None`, `epoch_timeout` piccolo (es. 100ms).
6. `AskLimitExceeded(n)`: WAT che chiama `ask` 3 volte con `max_ask_calls = 2` ⇒ `matches!(err, SandboxError::AskLimitExceeded(2))` — verifica che `e.chain()` preservi l'errore tipizzato.
7. `PayloadTooLarge`: `ask` con `req_len > max_ask_payload_bytes`.
8. `MissingExport`: modulo senza `guest_free`.
9. `GuestOutOfMemory`: WAT con `guest_alloc` che restituisce 0, input vuoto, risposta host non-empty.
10. `SandboxError::Host`: handler che restituisce `Err(HostError::BudgetExceeded)`.
11. `test_cache_corrupted_file_recompiles`: run con `cache_dir` (temp dir unica col PID), poi corrompere il `.cwasm` con `b"corrupted"`, nuovo engine ⇒ run OK, output `b"host_saw[b]"`.
12. `test_error_chain_preserves_typed_error` (come al punto 6).
13. `test_guest_free_called_on_response`: 10 run consecutive con input `msg{i}`, output `starts_with(b"host_saw[")`.
14. Cache hit: secondo `SandboxEngine::new` con stesso `cache_dir` riusa `.cwasm` senza errori.
15. Opzionale `guest_echo_e2e`: se l'env var `GUEST_ECHO_WASM` punta all'artefatto di guest-echo, eseguirlo (output `echo_result:host_saw[inspect:...]`); altrimenti skip silenzioso con `eprintln!` (la suite deve restare verde senza il target wasm32).

### Test di robustezza (v0.4, 16–22)

16. `test_request_payload_exactly_at_limit_ok`: input di lunghezza **esattamente**
    `max_ask_payload_bytes` ⇒ passa (il controllo è `> max`).
17. `test_request_payload_one_over_limit`: `max + 1` ⇒ `PayloadTooLarge { size, max }` esatti.
18. `test_response_payload_exactly_at_limit_ok`: handler che risponde **esattamente** `max` byte ⇒ passa.
19. `test_response_payload_one_over_limit`: risposta `max + 1` ⇒ `PayloadTooLarge` (lato risposta).
20. `test_memory_growth_bounded_by_limit`: `memory.grow` entro il limite ⇒ ok;
    oltre `max_memory_bytes` ⇒ **-1 (no trap)**.
21. `test_oversized_initial_memory_rejected`: modulo con memoria iniziale oltre il limite ⇒ `InvalidWasm` a `new`.
22. `test_concurrent_engines_share_cache_dir`: 4 thread × engine propri sullo stesso `cache_dir`
    (race sui tmp della cache + isolamento ticker).

# INTERFACCE DI FRUIZIONE (v0.5)

## crates/wasmbox-cli (CLI)

- Comando unico: `wasmbox-cli run <guest.wasm> [input] [--json]`;
- input: da argv, altrimenti da stdin pipe (agenti/script);
- `--json`: una riga su stdout `{"ok":true,"output":"…"}` oppure
  `{"ok":false,"error":"<snake_case kind>","message":"..."}` — mai altro su stdout;
- senza `--json`: output del guest su stdout, log su stderr;
- exit code deterministici: 0 ok · 2 uso/IO · 3 fuel · 4 timeout · 5 memoria
  guest · 6 ask limit/payload · 7 errore handler · 8 missing export · 9 wasm
  invalido · 10 esecuzione/engine;
- handler della CLI: **eco** (solo demo) — nessun protocollo extra, nessuna
  logica di dominio (i divieti della spec restano validi);
- tuning sandbox NON esposto (surface minima deliberatamente).

## crates/wasmbox-ui (UI minimale)

- Slint **senza systemfonts** (`default-features = false` con `compat-1-18`,
  `renderer-software`, `unsafe-single-threaded`, `libm`) — nessun fontconfig;
- screenshot: `wasmbox-ui --screenshot out.bmp [guest.wasm] [input]` —
  `MinimalSoftwareWindow` + `SoftwareRenderer.render(buffer, stride)` su pixel
  RGB; BMP 24-bit non compresso scritto a mano (54 header + righe bottom-up);
- esito visual: verde = run guest ok, rossa = fallita, grigia = mai eseguita;
- exit: 0 ok · 2 uso · 11 render vuoto · 12 sandbox fallita;
- richiede dimensione della finestra (`set_size`) dopo `show()`, altrimenti il
  render è vuoto (trappola nota).

## skills/wasmbox/SKILL.md (contract per agenti AI)

- invocazione, prerequisiti, tabella exit code, politica di default
  (`--json` sempre; non ritentare su fuel/timeout/OOM; guest = non fidato);
- limiti espliciti dichiarati (la CLI fa solo eco; dominio reale → crate host
  proprio con `HostHandler` + `wasmbox-core`).

# VALIDAZIONE FINALE (tutte obbligatorie)

- `cargo check --workspace`
- `cargo test -p wasmbox-core -- --nocapture`
- `cargo build -p guest-echo --target wasm32-unknown-unknown --release`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo fmt --check`
- Grep su `crates/wasmbox-core/src`: zero termini di dominio (backup, scan, robocopy, restore, sync); zero riferimenti a `wasmtime-wasi`, `serde_json`, `std::process`.
- v0.5: `wasmbox-cli --json` una run reale con output atteso `echo_result:…`;
  `wasmbox-ui --screenshot` produce BMP valido (magic `BM`, colori attesi)

# AMBIENTE

Nessun servizio esterno, nessuna variabile d'environment, nessun database, nessuna auth: il prodotto è un crate Rust autonomo. Il preview del container valida eseguendo la suite di test.
