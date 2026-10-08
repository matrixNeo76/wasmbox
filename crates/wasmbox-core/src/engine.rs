//! Il motore della sandbox: compilazione una-tolta, pooling allocator,
//! epoch ticker unico per engine e host function `ask`.
//!
//! `SandboxEngine` non sa nulla del dominio: inoltra bytes opachi al
//! [`HostHandler`] fornito dall'host e fa rispettare i limiti di risorsa.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use anyhow::anyhow;
use wasmtime::*;

use crate::config::{SandboxConfig, EPOCH_TICK};
use crate::error::{HostError, SandboxError};
use crate::memory::{pack_ptr_len, unpack_ptr_len};

/// Hash deterministico (non crittografico) del bytecode Wasm.
///
/// Sufficiente come cache key: due moduli identici producono lo stesso hash.
/// Non adatto a scopi di sicurezza.
fn compute_wasm_hash(wasm: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    wasm.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Stato per-store: limiti, contatore `ask` e puntatore all'handler.
struct StoreContext {
    config: SandboxConfig,
    calls_made: u32,
    /// Puntatore grezzo all'handler con lifetime cancellato.
    /// Validato solo per lo scope di `run()`: lo `Store` non sopravvive.
    handler: *mut (dyn HostHandler + 'static),
    limits: StoreLimits,
}

// SAFETY: `StoreContext` è usato solo all'interno di `run()`, dove `handler`
// è garantito vivo per tutta la durata dello Store. Non viene mai condiviso
// né migrato tra thread: il riferimento allo Store resta sul thread chiamante.
unsafe impl Send for StoreContext {}

/// Handler dell'host: l'unico punto in cui vive la logica di dominio.
///
/// `request` e la risposta sono bytes opachi: `wasmbox-core` non li interpreta.
pub trait HostHandler {
    fn ask(&mut self, request: &[u8]) -> Result<Vec<u8>, HostError>;
}

/// Esecutore Wasm isolato con limiti di risorsa.
///
/// La compilazione del modulo avviene una volta in [`SandboxEngine::new`]
/// (con cache `.cwasm` opzionale): [`SandboxEngine::run`] costa microsecondi.
pub struct SandboxEngine {
    engine: Engine,
    instance_pre: InstancePre<StoreContext>,
    config: SandboxConfig,
    ticker_stop: Arc<AtomicBool>,
    ticker_handle: Option<thread::JoinHandle<()>>,
}

impl SandboxEngine {
    /// Compila il modulo e prepara l'engine con i limiti della config.
    pub fn new(wasm: &[u8], config: SandboxConfig) -> Result<Self, SandboxError> {
        let mut w_config = Config::new();
        w_config.cranelift_opt_level(OptLevel::Speed);

        if config.max_fuel.is_some() {
            w_config.consume_fuel(true);
        }
        if config.epoch_timeout.is_some() {
            w_config.epoch_interruption(true);
        }

        // Pooling allocator: riuso di istanze per performance.
        let mut pool = PoolingAllocationConfig::default();
        pool.max_memory_size(config.max_memory_bytes);
        pool.total_memories(config.pool_size);
        pool.total_core_instances(config.pool_size);
        w_config.allocation_strategy(InstanceAllocationStrategy::Pooling(pool));

        let engine = Engine::new(&w_config).map_err(|e| SandboxError::EngineInit(e.to_string()))?;

        // Modulo: cache `.cwasm` con invalidazione automatica.
        let module = if let Some(cache_dir) = &config.cache_dir {
            std::fs::create_dir_all(cache_dir).map_err(|e| {
                SandboxError::EngineInit(format!("creazione cache_dir fallita: {e}"))
            })?;

            let hash = compute_wasm_hash(wasm);
            let cwasm_path = cache_dir.join(format!("{hash}.cwasm"));

            if cwasm_path.exists() {
                // Tentativo di caricamento nativo.
                // SAFETY: il file è prodotto da `Module::serialize()` sullo stesso
                // engine e con la stessa configurazione. Wasmtime valida internamente
                // versione e flag, restituendo Err in caso di mismatch.
                match unsafe { Module::deserialize_file(&engine, &cwasm_path) } {
                    Ok(m) => m,
                    Err(_) => {
                        // Cache corrotta o incompatibile: la eliminiamo e ricompiliamo.
                        let _ = std::fs::remove_file(&cwasm_path);
                        compile_and_cache(&engine, wasm, &cwasm_path, &hash, cache_dir)?
                    }
                }
            } else {
                compile_and_cache(&engine, wasm, &cwasm_path, &hash, cache_dir)?
            }
        } else {
            Module::new(&engine, wasm).map_err(|e| SandboxError::InvalidWasm(e.to_string()))?
        };

        // Linker: una sola import, `env::ask`.
        let mut linker: Linker<StoreContext> = Linker::new(&engine);
        linker
            .func_wrap("env", "ask", ask_host_function)
            .map_err(|e| SandboxError::EngineInit(e.to_string()))?;

        let instance_pre = linker
            .instantiate_pre(&module)
            .map_err(|e| SandboxError::EngineInit(format!("instantiate_pre: {e}")))?;

        // Ticker epoch: UNO solo per engine, avviato in `new()`, terminato in `Drop`.
        let (ticker_stop, ticker_handle) = if config.epoch_timeout.is_some() {
            let stop = Arc::new(AtomicBool::new(false));
            let engine_clone = engine.clone();
            let stop_clone = stop.clone();
            let handle = thread::Builder::new()
                .name("wasmbox-epoch-ticker".into())
                .spawn(move || {
                    while !stop_clone.load(Ordering::Relaxed) {
                        thread::sleep(EPOCH_TICK);
                        engine_clone.increment_epoch();
                    }
                })
                .map_err(|e| SandboxError::EngineInit(format!("ticker spawn: {e}")))?;
            (stop, Some(handle))
        } else {
            (Arc::new(AtomicBool::new(false)), None)
        };

        Ok(Self {
            engine,
            instance_pre,
            config,
            ticker_stop,
            ticker_handle,
        })
    }

    /// Esegue il guest con `input`, inoltrando le richieste `ask` a `handler`.
    ///
    /// `handler` è preso come `&mut dyn HostHandler` (non `Box`) così il
    /// chiamante può ispezionarne lo stato dopo la run.
    pub fn run(
        &self,
        input: &[u8],
        handler: &mut dyn HostHandler,
    ) -> Result<Vec<u8>, SandboxError> {
        let limits = StoreLimitsBuilder::new()
            .memory_size(self.config.max_memory_bytes)
            .instances(1)
            .build();

        // Il cast `as` non compila perché `handler` ha un lifetime anonimo,
        // mentre `StoreContext::handler` richiede `'static`: il `transmute`
        // cancella la lifetime del fat pointer.
        // SAFETY: l'invariante è che lo Store non sopravvive a questo scope:
        // il puntatore resta valido per tutta la durata di `run()` ed è
        // dereferenziato solo sul thread chiamante.
        let handler_ptr: *mut (dyn HostHandler + 'static) =
            unsafe { std::mem::transmute(handler as *mut dyn HostHandler) };

        let ctx = StoreContext {
            config: self.config.clone(),
            calls_made: 0,
            handler: handler_ptr,
            limits,
        };

        let mut store = Store::new(&self.engine, ctx);
        store.limiter(|s| &mut s.limits);

        if let Some(fuel) = self.config.max_fuel {
            store
                .set_fuel(fuel)
                .map_err(|e| SandboxError::EngineInit(e.to_string()))?;
        }
        if let Some(timeout) = self.config.epoch_timeout {
            let ticks = (timeout.as_millis() / EPOCH_TICK.as_millis()).max(1) as u64;
            store.set_epoch_deadline(ticks);
        }

        let instance = self
            .instance_pre
            .instantiate(&mut store)
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

        // --- Input ---
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

        // --- Esecuzione ---
        let packed = guest_run
            .call(&mut store, (in_ptr, in_len))
            .map_err(|e| map_guest_error(&e))?;

        let (out_ptr, out_len) = unpack_ptr_len(packed);
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

        Ok(output)
    }
}

impl Drop for SandboxEngine {
    fn drop(&mut self) {
        self.ticker_stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.ticker_handle.take() {
            let _ = h.join();
        }
    }
}

/// Compila il modulo e tenta di serializzarlo su disco in modo atomico.
///
/// Il fallimento della scrittura in cache non è fatale: il modulo compilato
/// viene comunque restituito.
fn compile_and_cache(
    engine: &Engine,
    wasm: &[u8],
    cwasm_path: &Path,
    hash: &str,
    cache_dir: &Path,
) -> Result<Module, SandboxError> {
    let module = Module::new(engine, wasm).map_err(|e| SandboxError::InvalidWasm(e.to_string()))?;

    if let Ok(serialized) = module.serialize() {
        // Nome tmp univoco per processo: evita race condition tra processi.
        let tmp: PathBuf = cache_dir.join(format!("{hash}.{}.tmp", std::process::id()));
        if std::fs::write(&tmp, &serialized).is_ok() {
            let _ = std::fs::rename(&tmp, cwasm_path);
        }
    }

    Ok(module)
}

/// Host function `ask`: l'unica capability che il guest può invocare.
///
/// Restituisce `anyhow::Result<i64>` (mai `Trap::new`) così gli errori
/// tipizzati sopravvivono al downcast in [`map_guest_error`].
fn ask_host_function(
    mut caller: Caller<'_, StoreContext>,
    req_ptr: i32,
    req_len: i32,
) -> anyhow::Result<i64> {
    // --- 1. Budget di chiamate ---
    let max_calls = caller.data().config.max_ask_calls;
    if caller.data().calls_made >= max_calls {
        return Err(anyhow!(SandboxError::AskLimitExceeded(max_calls)));
    }

    // --- 2. Limite payload richiesta ---
    let max_payload = caller.data().config.max_ask_payload_bytes;
    if (req_len as usize) > max_payload {
        return Err(anyhow!(SandboxError::PayloadTooLarge {
            size: req_len as usize,
            max: max_payload,
        }));
    }

    // --- 3. Lettura richiesta ---
    let memory = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| anyhow!(SandboxError::MissingExport("memory".into())))?;

    let req_bytes = {
        let data = memory.data(&caller);
        let start = req_ptr as usize;
        let end = start
            .checked_add(req_len as usize)
            .ok_or_else(|| anyhow!(SandboxError::MemoryAccess("ptr+len overflow".into())))?;
        if end > data.len() {
            return Err(anyhow!(SandboxError::MemoryAccess(
                "request out of bounds".to_string()
            )));
        }
        data[start..end].to_vec()
    };

    caller.data_mut().calls_made += 1;

    // --- 4. Handler (dominio interamente lato host) ---
    // SAFETY: `handler` è valido per l'intero scope di `run()`; lo Store
    // non sopravvive alla chiamata e tutto resta sul thread chiamante.
    let resp_bytes = unsafe { (*caller.data_mut().handler).ask(&req_bytes) }
        .map_err(|e| anyhow!(SandboxError::Host(e)))?;

    // --- 5. Limite payload risposta ---
    if resp_bytes.len() > max_payload {
        return Err(anyhow!(SandboxError::PayloadTooLarge {
            size: resp_bytes.len(),
            max: max_payload,
        }));
    }

    // --- 6. Allocazione buffer risposta nel guest ---
    let alloc_fn = caller
        .get_export("guest_alloc")
        .and_then(|e| e.into_func())
        .ok_or_else(|| anyhow!(SandboxError::MissingExport("guest_alloc".into())))?
        .typed::<u32, u32>(&caller)
        .map_err(|_| {
            anyhow!(SandboxError::MemoryAccess(
                "guest_alloc signature mismatch".to_string()
            ))
        })?;

    let resp_len = resp_bytes.len() as u32;
    let resp_ptr = alloc_fn
        .call(&mut caller, resp_len)
        .map_err(|e| anyhow!(SandboxError::Execution(format!("guest_alloc trap: {e}"))))?;

    // Allocazione maggiore di zero che restituisce 0 = out of memory:
    // mai scrivere all'offset 0.
    if resp_len > 0 && resp_ptr == 0 {
        return Err(anyhow!(SandboxError::GuestOutOfMemory));
    }

    // --- 7. Scrittura risposta ---
    {
        let data = memory.data_mut(&mut caller);
        let start = resp_ptr as usize;
        let end = start
            .checked_add(resp_bytes.len())
            .ok_or_else(|| anyhow!(SandboxError::MemoryAccess("ptr+len overflow".into())))?;
        if end > data.len() {
            return Err(anyhow!(SandboxError::MemoryAccess(
                "response out of bounds".to_string()
            )));
        }
        data[start..end].copy_from_slice(&resp_bytes);
    }

    Ok(pack_ptr_len(resp_ptr, resp_len) as i64)
}

/// Estrae un `SandboxError` tipizzato se la catena di `anyhow::Error`
/// lo contiene; altrimenti mappa il trap Wasmtime al variant più vicino.
///
/// Si ispeziona l'intera catena con `e.chain()`: `downcast_ref` sul solo
/// livello esterno non basta perché Wasmtime spesso incapsula gli errori
/// delle host function in un `wasmtime::Trap` o in un frame interno.
fn map_guest_error(e: &anyhow::Error) -> SandboxError {
    for cause in e.chain() {
        // 1. Errore tipizzato dal nostro host function.
        if let Some(se) = cause.downcast_ref::<SandboxError>() {
            return match se {
                SandboxError::AskLimitExceeded(n) => SandboxError::AskLimitExceeded(*n),
                SandboxError::PayloadTooLarge { size, max } => SandboxError::PayloadTooLarge {
                    size: *size,
                    max: *max,
                },
                SandboxError::Host(h) => SandboxError::Host(h.clone()),
                SandboxError::GuestOutOfMemory => SandboxError::GuestOutOfMemory,
                other => SandboxError::Execution(other.to_string()),
            };
        }

        // 2. Trap Wasmtime standard.
        if let Some(trap) = cause.downcast_ref::<wasmtime::Trap>() {
            return match trap {
                wasmtime::Trap::OutOfFuel => SandboxError::FuelExhausted,
                wasmtime::Trap::Interrupt => SandboxError::Timeout,
                wasmtime::Trap::UnreachableCodeReached => SandboxError::GuestOutOfMemory,
                _ => SandboxError::Execution(trap.to_string()),
            };
        }
    }

    SandboxError::Execution(e.to_string())
}
