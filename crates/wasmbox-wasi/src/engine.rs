//! `WasiEngine` — layer WASI opt-in sopra lo stesso profilo di limiti del
//! core (spec wasi-plan.md §4–5).
//!
//! DESIGN (semplice e verificabile):
//! - linker `wasi_snapshot_preview1` completo via `p1::add_to_linker_sync`
//!   (API supportata da wasmtime-wasi 49);
//! - **strict mode** (default): prima di montare il linker, l'engine
//!   verifica che TUTTE le import `wasi_snapshot_preview1` del modulo
//!   siano coperte dalle capability: se no → errore tipizzato
//!   `WasiDenied` (deny-by-default vero, spec test 1/3/5). Con
//!   `strict_imports = false` il guest parte e le syscall non coperte
//!   rispondono errno "negato" (wasmtime-wasi naturalmente NOENT su
//!   preopens assenti).
//! - stdio: stdin sempre vuoto; stdout/stderr catturati con
//!   `MemoryOutputPipe` (mai scritte sul processo host — invariante n.4);
//! - `env::ask` resta sul linker (invariante n.3) con la STESSA ABI del
//!   core; fuel/epoch/memoria/limits passano identici (invariante n.2).
//!
//! NOTE ABI: come il core, il guest deve esportare `memory`,
//! `guest_alloc(i32)->i32`, `guest_free(i32,i32)`, `guest_run(i32,i32)->i64`
//! (packed `(ptr << 32) | len`).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use wasmtime::{
    Config, Engine, InstanceAllocationStrategy, Linker, Module, OptLevel, PoolingAllocationConfig,
    Store, StoreLimits, StoreLimitsBuilder, Trap,
};
use wasmtime_wasi::p1;
use wasmtime_wasi::p1::WasiP1Ctx;
use wasmtime_wasi::p2::pipe::{MemoryInputPipe, MemoryOutputPipe};
use wasmtime_wasi::{FsPerms, WasiCtxBuilder};

use crate::caps::WasiCapabilities;
use crate::error::WasiError;
use wasmbox_core::{HostHandler, SandboxConfig, SandboxError};

/// Granularità del ticker epoch — stesso valore del core (`EPOCH_TICK`,
/// lì `pub(crate)`; duplicato qui con commento).
const EPOCH_TICK_MILLIS: u64 = 10;

/// Hash del bytecode (stessa scelta del core: DefaultHasher, non crittografico).
fn compute_wasm_hash(wasm: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    wasm.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Stato per-store: handler `ask`, limiti, ctx WASI Preview 1.
struct WasiStoreContext {
    handler: *mut (dyn HostHandler + 'static),
    limits: StoreLimits,
    wasi: WasiP1Ctx,
}

// SAFETY: identico invariante del core — lo Store non sopravvive a `run()`
// e il puntatore all'handler è dereferenziato solo sul thread chiamante;
// nessun dato attraversa thread (run + wasi restano sul chiamante).
unsafe impl Send for WasiStoreContext {}
// Il `Sync` serve alla bound `Send + Sync` della closure di
// `add_to_linker_sync`; l'accessor gira comunque solo sul chiamante.
unsafe impl Sync for WasiStoreContext {}

/// Host function `ask` — stessa ABI del core.
fn ask_host_function(
    mut caller: wasmtime::Caller<'_, WasiStoreContext>,
    req_ptr: i32,
    req_len: i32,
) -> wasmtime::Result<i64> {
    if req_ptr < 0 || req_len < 0 {
        return Err(SandboxError::MemoryAccess("ptr/len negativi in ask".into()).into());
    }
    let memory = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| SandboxError::MissingExport("memory".into()))?;

    let req_bytes = {
        let data = memory.data(&caller);
        let start = req_ptr as usize;
        let end = start
            .checked_add(req_len as usize)
            .ok_or_else(|| SandboxError::MemoryAccess("ptr+len overflow in ask".into()))?;
        if end > data.len() {
            return Err(SandboxError::MemoryAccess("request out of bounds".into()).into());
        }
        data[start..end].to_vec()
    };

    // SAFETY: il puntatore resta valido per l'intero scope di `run()`, è
    // dereferenziato solo sul thread chiamante (heredity del core).
    let resp_bytes =
        unsafe { (*caller.data_mut().handler).ask(&req_bytes) }.map_err(SandboxError::Host)?;

    let len = resp_bytes.len() as u32;
    if len == 0 {
        // Nessuna risposta: packed(0, 0) = 0.
        return Ok(0);
    }

    // La risposta usa `guest_alloc` del guest (stessa ABI del core).
    let alloc_fn = caller
        .get_export("guest_alloc")
        .and_then(|e| e.into_func())
        .ok_or_else(|| SandboxError::MissingExport("guest_alloc".into()))?
        .typed::<u32, u32>(&caller)?;
    let ptr = alloc_fn
        .call(&mut caller, len)
        .map_err(|e| SandboxError::Execution(format!("guest_alloc trap: {e}")))?;
    if ptr == 0 {
        return Err(SandboxError::GuestOutOfMemory.into());
    }

    {
        let data = memory.data_mut(&mut caller);
        let start = ptr as usize;
        let end = start
            .checked_add(len as usize)
            .ok_or_else(|| SandboxError::MemoryAccess("response ptr+len overflow".into()))?;
        if end > data.len() {
            return Err(SandboxError::MemoryAccess("response out of bounds".into()).into());
        }
        data[start..end].copy_from_slice(&resp_bytes);
    }

    let packed = ((ptr as u64) << 32) | (len as u64);
    Ok(packed as i64)
}

/// Mappa trap Wasmtime → errore tipizzato (fuel/timeout), come il core
/// (`map_guest_error` lì; `proc_exit` → Execution con messaggio).
fn map_guest_error(e: &wasmtime::Error) -> SandboxError {
    for cause in e.chain() {
        if let Some(trap) = cause.downcast_ref::<Trap>() {
            return match trap {
                Trap::OutOfFuel => SandboxError::FuelExhausted,
                Trap::Interrupt => SandboxError::Timeout,
                _ => SandboxError::Execution(trap.to_string()),
            };
        }
    }
    SandboxError::Execution(e.to_string())
}

/// Esecutore Wasm con WASI Preview 1 opt-in sopra i limiti del core.
pub struct WasiEngine {
    linker: Linker<WasiStoreContext>,
    module: Module,
    engine: Engine,
    config: SandboxConfig,
    capabilities: WasiCapabilities,
    /// Syscall WASI del modulo NON coperte dalle capability (strict mode:
    /// motivo del fallimento del build; diagnosi a runtime).
    denied_imports: Vec<String>,
    ticker_stop: Arc<AtomicBool>,
}

impl WasiEngine {
    /// Compila il modulo e prepara un engine WASI capability-gated.
    ///
    /// Fallisce con `SandboxError::EngineInit(… WasiDenied …)` se il guest
    /// importa syscall non coperte dalle capability (strict, default) e
    /// `PreopenInvalid` se una dir host non esiste come directory.
    pub fn new(
        wasm: &[u8],
        capabilities: &WasiCapabilities,
        config: SandboxConfig,
    ) -> Result<Self, SandboxError> {
        Self::build(wasm, capabilities, config, true)
    }

    /// Variante non-strict: il guest con import "extra" istanzia e le
    /// syscall non coperte rispondono errno (NOENT/ACCEs come wasmrun
    /// wasmtime); utile per guest standard che parlano Preview 1 pieno.
    pub fn new_permissive(
        wasm: &[u8],
        capabilities: &WasiCapabilities,
        config: SandboxConfig,
    ) -> Result<Self, SandboxError> {
        Self::build(wasm, capabilities, config, false)
    }

    fn build(
        wasm: &[u8],
        capabilities: &WasiCapabilities,
        config: SandboxConfig,
        strict: bool,
    ) -> Result<Self, SandboxError> {
        // Preopen valide PRIMA di tutto (fail fast).
        for p in &capabilities.preopens {
            let ok = std::fs::metadata(&p.host_path)
                .map(|m| m.is_dir())
                .unwrap_or(false);
            if !ok {
                return Err(SandboxError::EngineInit(
                    WasiError::PreopenInvalid(p.host_path.display().to_string()).to_string(),
                ));
            }
        }

        // ① Strict import check (deny-by-default, spec test 1/3/5).
        let strict_denied = {
            // Preview-check su module temporaneo: Module::new su
            // Engine "vuoto" non compila nulla (costo di parse puro).
            let chk_engine = {
                let mut c = Config::new();
                c.cranelift_opt_level(OptLevel::Speed);
                Engine::new(&c).map_err(|e| SandboxError::EngineInit(e.to_string()))?
            };
            let m = Module::new(&chk_engine, wasm)
                .map_err(|e| SandboxError::InvalidWasm(e.to_string()))?;
            denied_imports_list(&m, capabilities)
        };
        if strict && !strict_denied.is_empty() {
            return Err(SandboxError::EngineInit(
                WasiError::Denied(format!(
                    "il guest importa syscall WASI non in allow-list: [{}]",
                    strict_denied.join(", ")
                ))
                .to_string(),
            ));
        }
        let denied_imports = strict_denied;

        // Cache `.cwasm` (stessa logica del core).
        let engine = Self::engine_for(&config)?;
        let module = if let Some(cache_dir) = &config.cache_dir {
            std::fs::create_dir_all(cache_dir).map_err(|e| {
                SandboxError::EngineInit(format!("creazione cache_dir fallita: {e}"))
            })?;
            let hash = compute_wasm_hash(wasm);
            let cwasm_path = cache_dir.join(format!("{hash}.cwasm"));
            if cwasm_path.exists() {
                // SAFETY: file prodotto da `Module::serialize()` sullo
                // stesso engine+config; Wasmtime valida versione/flag e
                // restituisce Err su mismatch (identico al core).
                match unsafe { Module::deserialize_file(&engine, &cwasm_path) } {
                    Ok(m) => m,
                    Err(_) => {
                        let _ = std::fs::remove_file(&cwasm_path);
                        compile_and_cache(&engine, wasm, &cwasm_path)?
                    }
                }
            } else {
                compile_and_cache(&engine, wasm, &cwasm_path)?
            }
        } else {
            Module::new(&engine, wasm).map_err(|e| SandboxError::InvalidWasm(e.to_string()))?
        };

        // ② Linker: `env::ask` + WASI Preview 1 sync.
        let mut linker: Linker<WasiStoreContext> = Linker::new(&engine);
        linker
            .func_wrap("env", "ask", ask_host_function)
            .map_err(|e| SandboxError::EngineInit(e.to_string()))?;
        p1::add_to_linker_sync(&mut linker, |t: &mut WasiStoreContext| &mut t.wasi)
            .map_err(|e| SandboxError::EngineInit(format!("wasi linker: {e}")))?;

        // ③ Ticker epoch: UNO per engine (divieto thread-per-run, operative).
        let ticker_stop = Arc::new(AtomicBool::new(false));
        if config.epoch_timeout.is_some() {
            let engine_clone = engine.clone();
            let stop = ticker_stop.clone();
            std::thread::Builder::new()
                .name("wasmbox-wasi-epoch-ticker".into())
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        std::thread::sleep(std::time::Duration::from_millis(EPOCH_TICK_MILLIS));
                        engine_clone.increment_epoch();
                    }
                })
                .map_err(|e| SandboxError::EngineInit(format!("ticker spawn: {e}")))?;
        }

        Ok(Self {
            linker,
            module,
            engine,
            config,
            capabilities: capabilities.clone(),
            denied_imports,
            ticker_stop,
        })
    }

    fn engine_for(config: &SandboxConfig) -> Result<Engine, SandboxError> {
        let mut w_config = Config::new();
        w_config.cranelift_opt_level(OptLevel::Speed);
        if config.max_fuel.is_some() {
            w_config.consume_fuel(true);
        }
        if config.epoch_timeout.is_some() {
            w_config.epoch_interruption(true);
        }
        let mut pool = PoolingAllocationConfig::default();
        pool.max_memory_size(config.max_memory_bytes);
        pool.total_memories(config.pool_size);
        pool.total_core_instances(config.pool_size);
        w_config.allocation_strategy(InstanceAllocationStrategy::Pooling(pool));
        Engine::new(&w_config).map_err(|e| SandboxError::EngineInit(e.to_string()))
    }

    /// Syscall WASI non coperte dalle capability (diagnosi e test).
    pub fn denied_imports(&self) -> &[String] {
        &self.denied_imports
    }

    /// Capability con cui l'engine è stato creato.
    pub fn capabilities(&self) -> &WasiCapabilities {
        &self.capabilities
    }

    /// Esegue il guest: ritorna `(output, stdout_captured)`.
    pub fn run(
        &self,
        input: &[u8],
        handler: &mut dyn HostHandler,
    ) -> Result<(Vec<u8>, Vec<u8>), SandboxError> {
        let limits = StoreLimitsBuilder::new()
            .memory_size(self.config.max_memory_bytes)
            .instances(1)
            .build();

        // Il cast `as` non compila (lifetime anonimo vs `'static`): stesso
        // transmute confinato del core, invariante documentato (SAFETY).
        let handler_ptr: *mut (dyn HostHandler + 'static) = unsafe {
            std::mem::transmute::<*mut dyn HostHandler, *mut (dyn HostHandler + 'static)>(
                handler as *mut dyn HostHandler,
            )
        };

        // stdout/stderr catturati: teniamo un clone del pipe per leggere
        // il contenuto a fine run (identità condivisa con ciò che il guest
        // scrive — MemoryOutputPipe è Clone e condivide il buffer Arc).
        let stdout_pipe = MemoryOutputPipe::new(1024 * 1024);
        // stdin sempre vuoto: MemoryInputPipe con buffer iniziale d'0 byte
        // (Into<Bytes> su &[] — niente dipendenze dirette su `bytes`).
        let stdin_pipe = MemoryInputPipe::new(Vec::new());

        let ctx = WasiStoreContext {
            handler: handler_ptr,
            limits,
            wasi: build_wasi(&self.capabilities, stdout_pipe.clone(), stdin_pipe),
        };

        let mut store = Store::new(&self.engine, ctx);
        store.limiter(|s| &mut s.limits);

        if let Some(fuel) = self.config.max_fuel {
            store
                .set_fuel(fuel)
                .map_err(|e| SandboxError::EngineInit(e.to_string()))?;
        }
        if let Some(timeout) = self.config.epoch_timeout {
            let ticks = (timeout.as_millis() as u64 / EPOCH_TICK_MILLIS).max(1);
            store.set_epoch_deadline(ticks);
        }

        let instance = self
            .linker
            .instantiate(&mut store, &self.module)
            .map_err(|e| SandboxError::Execution(format!("instantiate: {e}")))?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| SandboxError::MissingExport("memory".into()))?;

        let guest_alloc = instance
            .get_typed_func::<u32, u32>(&mut store, "guest_alloc")
            .map_err(|_| SandboxError::MissingExport("guest_alloc".into()))?;
        let guest_free = instance
            .get_typed_func::<(u32, u32), ()>(&mut store, "guest_free")
            .map_err(|_| SandboxError::MissingExport("guest_free".into()))?;
        let guest_run = instance
            .get_typed_func::<(u32, u32), u64>(&mut store, "guest_run")
            .map_err(|_| SandboxError::MissingExport("guest_run".into()))?;

        // --- Input (identico al core) ---
        let (in_ptr, in_len) = if input.is_empty() {
            (0u32, 0u32)
        } else {
            let len = input.len() as u32;
            let ptr = guest_alloc
                .call(&mut store, len)
                .map_err(|e| SandboxError::Execution(format!("guest_alloc(input): {e}")))?;
            if ptr == 0 {
                return Err(SandboxError::GuestOutOfMemory);
            }
            let data = memory.data_mut(&mut store);
            let start = ptr as usize;
            let end = start
                .checked_add(len as usize)
                .ok_or_else(|| SandboxError::MemoryAccess("input ptr+len overflow".into()))?;
            if end > data.len() {
                return Err(SandboxError::MemoryAccess("input out of bounds".into()));
            }
            data[start..end].copy_from_slice(input);
            (ptr, len)
        };

        // --- Esecuzione (trap tipizzate fuel/epoch) ---
        let packed = guest_run
            .call(&mut store, (in_ptr, in_len))
            .map_err(|e| map_guest_error(&e))?;
        let (out_ptr, out_len) = ((packed >> 32) as u32, (packed & 0xFFFF_FFFF) as u32);

        let output = {
            let data = memory.data(&store);
            let start = out_ptr as usize;
            let end = start
                .checked_add(out_len as usize)
                .ok_or_else(|| SandboxError::MemoryAccess("output ptr+len overflow".into()))?;
            if end > data.len() {
                return Err(SandboxError::MemoryAccess("output out of bounds".into()));
            }
            data[start..end].to_vec()
        };

        if in_len > 0 {
            let _ = guest_free.call(&mut store, (in_ptr, in_len));
        }
        if out_len > 0 {
            let _ = guest_free.call(&mut store, (out_ptr, out_len));
        }

        // stdout catturato: il pipe condiviso col ctx, letto a run conclusa
        // (il guest non può più scrivere una volta tornati qui).
        let stdout_bytes = stdout_pipe.contents().to_vec();
        Ok((output, stdout_bytes))
    }
}

impl Drop for WasiEngine {
    fn drop(&mut self) {
        self.ticker_stop.store(true, Ordering::Relaxed);
    }
}

// ─── Helper privati ──────────────────────────────────────────────────────────

fn compile_and_cache(
    engine: &Engine,
    wasm: &[u8],
    cwasm_path: &std::path::Path,
) -> Result<Module, SandboxError> {
    let module = Module::new(engine, wasm).map_err(|e| SandboxError::InvalidWasm(e.to_string()))?;
    if let Ok(serialized) = module.serialize() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let parent = cwasm_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let tmp: std::path::PathBuf = parent.join(format!(
            "{}.{stamp}.tmp",
            cwasm_path.file_name().unwrap_or_default().to_string_lossy()
        ));
        if std::fs::write(&tmp, &serialized).is_ok() {
            let _ = std::fs::rename(&tmp, cwasm_path);
        }
    }
    Ok(module)
}

/// Syscall WASI del modulo NON in allow-list (spec §3 tabella subset).
fn denied_imports_list(module: &Module, caps: &WasiCapabilities) -> Vec<String> {
    let has_preopens = caps.has_preopens();
    let write_allowed = has_preopens && !caps.all_readonly();
    let mut denied = Vec::new();
    for imp in module.imports() {
        if imp.module() != "wasi_snapshot_preview1" {
            continue;
        }
        let n = imp.name();
        let granted = match n {
            // stdio: sempre concessi — il ctx li mappa su pipes memoria
            // catturate (stdout/stderr) e stdin vuoto (invariante n.4).
            "fd_write" | "fd_fdstat_get" | "fd_close" | "fd_seek" => true,
            // File I/O: concessa se ci sono preopens; il ctx naturalizza
            // ogni accesso fuori dalle aree preaperte (errno).
            "path_open"
            | "fd_read"
            | "fd_fdstat_set_flags"
            | "fd_prestat_get"
            | "fd_prestat_dir_name"
            | "fd_filestat_get"
            | "path_filestat_get"
            | "fd_readdir"
            | "fd_advise"
            | "fd_allocate"
            | "fd_datasync"
            | "fd_sync"
            | "fd_renumber"
            | "fd_tell" => has_preopens,
            // Write su directory host: richiede preopen read-write.
            "path_create_directory"
            | "path_remove_directory"
            | "path_unlink_file"
            | "path_rename"
            | "path_symlink"
            | "path_link"
            | "fd_filestat_set_size"
            | "fd_filestat_set_times" => write_allowed,
            // Tempo ed entropia: opt-in espliciti (non deterministici).
            "clock_time_get" => caps.allow_clock,
            "random_get" => caps.allow_random,
            // Sempre (termina la run, mappata a errore tipizzato).
            "proc_exit" => true,
            _ => false,
        };
        if !granted {
            denied.push(n.to_string());
        }
    }
    denied
}

/// Ctx WASI capabilit-gated: stdin vuoto, stdout/stderr catturati,
/// preopens solo se dichiarati, niente env/argv (deny di default).
fn build_wasi(
    caps: &WasiCapabilities,
    stdout: MemoryOutputPipe,
    stdin: MemoryInputPipe,
) -> WasiP1Ctx {
    let mut b = WasiCtxBuilder::new();

    // stdio: stdout/stderr in memory pipes catturate, stdin vuoto.
    b.stdout(stdout);
    b.stderr(MemoryOutputPipe::new(64 * 1024));
    b.stdin(stdin);

    // Preopen (solo quelle dichiarate).
    for p in &caps.preopens {
        let perms = if p.readonly {
            FsPerms::ReadOnly
        } else {
            FsPerms::ReadWrite
        };
        let _ = b.preopened_dir(&p.host_path, &p.guest_alias, perms);
    }

    b.build_p1()
}
