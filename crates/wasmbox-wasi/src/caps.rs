//! Capability model WASI — spec wasi-plan.md §3.
//!
//! Struttura dichiarativa: l'host elenca le capability che intende darmi
//! (cioè dare al guest) al build dell'engine. `WasiCapabilities::default()`
//! = ZERO capability (deny-by-default): un guest che importa WASI con
//! default non istanzia (vedi `engine::strict_import_check`).

use std::path::PathBuf;

/// Gestione di uno stream stdio del guest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StdioMode {
    /// Il guest vede lo stream ma esso è un sink vuoto (stdin) —
    /// v0.8: stdin è SEMPRE vuoto; questo variant esiste per completezza.
    #[default]
    Denied,
    /// I byte scritti dal guest finiscono in un buffer di memoria
    /// (`MemoryOutputPipe`) restituito da `WasiEngine::run`.
    Captured,
}

/// Una directory host preaperta per il guest, con alias nel namespace WASI.
///
/// Il guest vede SOLO `guest_alias`: non può risalire al path host reale,
/// né uscire dall'area via `path_open` (wasmtime-wasi canonicalizza dentro
/// la preopen: `../` e symlink esterni restano confinati).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preopen {
    /// Path reale sul filesystem host (deve esistere ed essere una dir).
    pub host_path: PathBuf,
    /// Come il guest la vede, es. "/data" (mai il path host).
    pub guest_alias: String,
    /// Solo lettura: le operazioni di scrittura ottengono errno ACCEs.
    pub readonly: bool,
}

/// Capability WASI dichiarate dall'host al build dell'engine.
///
/// Default: zero preopen, zero clock/random → qualsiasi guest che
/// importa no file I/O, `clock_time_get` o `random_get` NON istanzia
/// (deny-by-default, spec test 1/3/5).
#[derive(Debug, Clone, Default)]
pub struct WasiCapabilities {
    /// Directory preaperte (zero di default).
    pub preopens: Vec<Preopen>,
    /// stdin: sempre vuoto in v0.8 (campo per future estensioni).
    pub stdin_mode: StdioMode,
    /// stdout: Captured di default (buffer memoria, mai processo host).
    pub stdout_mode: StdioMode,
    /// stderr: Captured di default.
    pub stderr_mode: StdioMode,
    /// Espone `clock_time_get` (tempo wall-clock reale, non deterministico
    /// → opt-in esplicito).
    pub allow_clock: bool,
    /// Espone `random_get` (entropia reale, non deterministica → opt-in
    /// esplicito).
    pub allow_random: bool,
}

impl WasiCapabilities {
    /// Zero capability: il guest non può fare I/O file, né tempo/entropia.
    pub fn denied() -> Self {
        Self::default()
    }

    /// Directory read-only preaperta con alias guest.
    pub fn preopen_readonly(&mut self, host_path: &str, guest_alias: &str) -> &mut Self {
        self.preopens.push(Preopen {
            host_path: PathBuf::from(host_path),
            guest_alias: guest_alias.to_string(),
            readonly: true,
        });
        self
    }

    /// Directory read-write preaperta (concedere SOLO per necessità esplicita).
    pub fn preopen_readwrite(&mut self, host_path: &str, guest_alias: &str) -> &mut Self {
        self.preopens.push(Preopen {
            host_path: PathBuf::from(host_path),
            guest_alias: guest_alias.to_string(),
            readonly: false,
        });
        self
    }

    /// Abilita la cattura di stdout (già default; documentale esplicito).
    pub fn stdout_captured(&mut self) -> &mut Self {
        self.stdout_mode = StdioMode::Captured;
        self
    }

    /// True se tutti i preopen sono read-only.
    pub fn all_readonly(&self) -> bool {
        self.preopens.iter().all(|p| p.readonly)
    }

    /// True se esiste ALMENO un preopen (abilita il famiglio file I/O).
    pub fn has_preopens(&self) -> bool {
        !self.preopens.is_empty()
    }
}
