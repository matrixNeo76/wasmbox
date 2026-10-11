//! Layer WASI opt-in per wasmbox — spec: [`docs/wasi-plan.md`](../../docs/wasi-plan.md).
//!
//! Principi (non negoziabili, spec §4):
//! 1. **Default = denied**: `WasiCapabilities::default()` espone zero
//!    capability. Un guest che importa WASI con default NON istanzia
//!    (`WasiError::Denied` – strict mode).
//! 2. **Intatti tutti i limiti wasmbox**: fuel/epoch/memoria/budget `ask`
//!    identici al modello puro (stessa `SandboxConfig` del core).
//! 3. **`env::ask` resta disponibile**: ABI identica (`(i32,i32)->i64`).
//! 4. **Niente logica di dominio**: il crate fa solo I/O generico.
//!
//! Subset Preview 1 della v0.8 (strict): file I/O su preopen
//! (read-only di default), `proc_exit` sempre, `clock_time_get` e
//! `random_get` solo opt-in espliciti. Tutto il resto (sock*, poll,
//! environ, argv, proc_*) NON è in allow-list.
//!
//! ABI del guest: come il core (`memory`, `guest_alloc`, `guest_free`,
//! `guest_run`), con import WASI aggiuntivi opzionali.

pub mod caps;
pub mod engine;
pub mod error;

pub use caps::{Preopen, StdioMode, WasiCapabilities};
pub use engine::WasiEngine;
pub use error::WasiError;
