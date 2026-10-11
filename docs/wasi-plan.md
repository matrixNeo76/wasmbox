---
type: Specification
title: "wasmbox — piano WASI opt-in (v0.8, APPROVATO e IMPLEMENTATO)"
description: "Spec per il layer WASI opzionale separato da wasmbox-core: gli utenti scelgono tra il modello deny-by-default puro (solo ask) e un modello flessibile ma controllato (WASI Preview 1 subset, capability-gated). Stato: IMPLEMENTATO 2026-10-11, 7/7 test verdi."
resource: "docs/wasi-plan.md"
tags: ["spec", "wasi", "sandbox", "capabilities", "plan"]
updated: "2026-10-11"
---

# PIANO WASI OPT-IN — wasmbox (v0.8, APPROVATO E IMPLEMENTATO)

> **Stato 2026-10-11: IMPLEMENTATO.** L'utente ha confermato ("confermo puoi
> procedere"); crate `crates/wasmbox-wasi` online con 7/7 test di
> accettazione verdi. Le deviazioni rispetto al draft sono dichiarate in
> §10 (seguito).

---

## 1) Perché e cosa NON cambia

**Perché**: il divieto WASI nel proteggere il "deny-by-default" rende wasmbox
immediato per i tool generati da LLM, ma esclude progetti che richiedono un
guest *standard* (legge un file di dati, scrive su stdout) senza volere il
modello all-or-nothing di wasmtime-wasi. Un layer **opt-in, separato** dà agli
utenti la scelta senza toccare il modello puro.

**Non cambia** (vincoli restano validi, aggiornati):

- `wasmbox-core` NON dipende da `wasmtime-wasi`: divieto blueprint n.1 resta,
  il layer vive in un NUOVO crate;
- la CLI/UI/HTTP/FFI esistenti restano deny-by-default (nessun flag che le
  abiliti a WASI: l'adozione del layer è una scelta esplicita dell'applicativo
  host che compone i crate);
- fuel/epoch/`StoreLimits`/budget di chiamate `ask` NON vengono meno: WASI si
  aggiunge SOPRA i limiti, mai come alternativa.

## 2) Dove vive il codice

```
crates/wasmbox-wasi/          # nuovo crate membri del workspace
├── Cargo.toml                # deps: wasmbox-core (path) + wasmtime con feature "wasi"
└── src/
    ├── lib.rs                # pub mod wasi_ctx, wasi_linker (subset)
    ├── ctx.rs                # WasiCtxBuilder-like: capability dichiarate a build
    └── matcher.rs            # pattern host (glob ristretto) per preopen
```

- **Feature opt-in nel crate wasmbox-wasi**, NON feature del core: il core
  resta intatto (grep `wasmtime-wasi` su `src/` = 0, come da
  validazione blueprint);
- dipendenza: `wasmtime` con feature `wasi` soltanto nel crate
  `wasmbox-wasi` (self-contained; il resto del workspace non la trascina);
- niente `serde`/`serde_json` (coerente col divieto di dominio): la config
  capability è una struct Rust tipizzata.

## 3) Capability model

Le capability WASI sono **non ereditate**: l'host le dichiara al build del
`WasiEngine` al momento della costruzione e NON ci sono default che le
attivano implicitamente.

```rust
// config lato host — tutte le varianti "assenti" di default = denied
pub struct WasiCapabilities {
    // preopen di directory (chiavi costanti; niente path arbitrari dal guest)
    preopens: Vec<Preopen>,         // { host_path: PathBuf, guest_alias: String, readonly: bool }
    // stdio: "null" | "captured" (captured = l'host riceve i bytes, opaco)
    stdin:  StdinMode,              // default HandsOff (nessun stdin)
    stdout: StdoutMode,             // default Captured (ritorno nel run)
    stderr: StdoutMode,             // default Captured
    // subset allow-list: nessun import implicito all'inizio
    allow_clock:   bool,            // fd_clock_get / time — solo tempo monotonic / UTC reso deterministico
    allow_random:  bool,            // random_get — opzionale: random deterministico (WasiCtx opzionale)
    // ⚠ MAI esposti in v0.8: sock, poll (fd_poll di rete), environ, argv, proc_*
}
```

**Subset Preview 1 in v0.8** (allow-list esplicita: tutto il resto è negato):

| Import WASI allow-listed                        | Condizione |
|---|---|
| `fd_read`, `fd_write` (solo su fd preopen o stdio) | capability per-guest, preopen match |
| `fd_close`, `fd_seek`, `fd_fdstat_get`          | solo fd già possessi |
| `path_open`                                     | solo dentro un preopen, read-only di default, flag di scrittura rifiutati esplicitamente + log |
| `fd_prestat_get` / `fd_prestat_dir_name`        | enumerazione dei preopen dichiarati |
| `clock_time_get`                                | solo `allow_clock` |
| `random_get`                                    | solo `allow_random` |
| `proc_exit`                                     | sempre (catch, mappa a errore tipizzato) |
| TUTTO IL RESTO (sock*, poll, environ, argv, proc_*) | **linker stub → errore `WasiDenied`** |

## 4) Invarianti (non negoziabili)

1. **Default = denied**: `WasiCapabilities::default()` ha zero preopen,
   stdio "null", nessun flag on: la configurazione di default conserva
   l'equivalente del modello puro.
2. **Intatti tutti i limiti wasmbox**: fuel/epoch/memoria passano SEMPRE
   insieme a WasiCtx (StoreLimits NON sostituito; aggiunto).
3. **Ask resta disponibile**: `env::ask` segue nel linker; il guest può
   mescolare file I/O WASI e `ask` opaca (il "ponte" verso il backend
   esterno).
4. **Il runtime NON interpreta i dati**: i preopen sono directory host
   dichiarate esplicitamente; la sandbox non implementa logica di dominio:
   quella resta nell'host applicativo. WASI serve solo per IO/CLI standard.
5. **Errore tipizzato**: nuovo errore `WasiDenied{cap: &'static str}`
   definito nel crate wasi (mai in core: error.rs non cambia).
6. **Niente dominio in wasi**: il crate gira solo IO generico —
   zero termini di dominio, niente stato applicativo.
7. **Divieto thread-per-run resta**: il ctx WASI va su `Store::data()`, un
   thread ticker unico per engine come oggi.

## 5) API proposta (v0.8, draft)

```rust
use wasmbox_core::{SandboxConfig, SandboxError, HostHandler};
use wasmbox_wasi::{WasiCapabilities, WasiEngine};

let caps = WasiCapabilities::builder()
    .preopen_readonly("/data", "data:/")     // solo dir note a build time
    .stdout_capture()                        // il guest può stampare
    .allow_clock(false)                      // deterministica
    .build()?;

let engine = WasiEngine::new(&wasm_bytes, &caps,
        SandboxConfig { max_fuel: Some(10_000_000), ..Default::default() })?;

let (out, stdout) = engine.run(input, &mut handler)?;  // stdout = Vec<u8> captured
```

- `WasiEngine` incapsula `SandboxEngine`: riusa la compile cache `.cwasm`
  (stesso layout, stesso driver) e NON espone il component model;
- `run(input, handler)` firma identica a core + stdout captured (non
  `println!` diretto lato host: sempre passivo).

## 6) Test richiesti (accettazione)

| # | Test | Atteso |
|---|---|---|
| 1 | default = denied: guest con import WASI, zero caps | `WasiDenied` (o import stub fallback err) |
| 2 | fd_read su preopen OK (file ristretto) | leggi byte dal file |
| 3 | `path_open` FUORI preopen | `WasiDenied` |
| 4 | `path_open` scrivendo su preopen readonly | `WasiDenied` |
| 5 | `random_get` senza allow | `WasiDenied` |
| 6 | stdio captured | stdout ritorno in `run`, niente escape sul processo host |
| 7 | fuel+epoch CONTINUANO a funzionare con WASI | fuel bomb → `FuelExhausted` / epoch → `Timeout` |
| 8 | `ask` c'è ancora nel guest WASI | round-trip come scenario B |
| 9 | benchmark rappresentativo (layer on/off) | overhead WASI ≤ 20% vs baseline |
| 10 | grep su `crates/wasmbox-core/src`: zero `wasmtime-wasi`/`wasi` | grep fallito = PASS |

## 7) Rischi noti (dichiarati)

- **erosione del messaggio di sicurezza**: se WASI va in on troppo facilmente,
  il claim "non c'è WASI" va gestito: documentazione chiara "deny-by-default
  resta il default; WASI è opt-in esplicito per-guest" nel README;
- **wasmtime Preview 1 vs Preview 2**: qui si parte con Preview 1 via
  `wasi-common` (Preview 1); Preview 2/component model è FUTURO e non fa parte di questa
  spec;
- **path traversal**: `path_open` collaudato con attack vectors noti
  (`../`, symlinks, traversal via alias) nel test suite 3/4.
- **dipendenze**: `wasmtime + feature wasi` accorpa il peso di build del crate
  wasi solo (il core non subisce); dipendenze fuori dalla lista blueprint
  sono consentite SOLO nel crate wasi (annotate).

## 8) Ordine di implementazione (dopo l'approvazione)

1. Crate vuoto `crates/wasmbox-wasi` nel workspace → `cargo check --workspace`.
2. `WasiCapabilities` + stub linker deny-all (primo passo: tutto negato).
3. Test 1, 3, 4, 5, 7, 10 (i divieti — le vere garanzie).
4. `fd_read/fd_write/path_open` su preopen + test 2, 6, 8.
5. Benchmark (test 9) + docs (README, integration.md §, log, ROADMAP).
6. build guest wasm32 di prova + baseline 27/27 + CI + CodeQL.

## 9) Cosa NON è in scope (v0.8)

- Preview 2 / component model;
- socket virtual networking (sock_*) — mai già mappati;
- WASI CLI e collisioni con l'API pubblica esistente (la CLI è e resta
  deny-by-default);
- esecuzione di guest MISTI con import non dichiarati;
- auto-derivation delle capability dai dati di runtime (nessun guess).

## 10) Deviazioni dal draft (onesto, post-implementazione)

Il codice realizzato differisce dal draft in 3 punti, per semplicità e
verificabilità (nessuna perdita di garanzia):

1. **Strict mode = fallire il build, non errno a runtime.** Il draft
   prevedeva linker "stub → WasiDenied"; realizzato: l'engine rifuta il
   modulo che importa syscall non in allow-list con
   `SandboxError::EngineInit(WasiError::Denied …)` al `new()` (test 1/3/5
   falliscono lì). Più stretto del draft: nessuna superficie si nasconde;
   variante permissiva disponibile (`WasiEngine::new_permissive`).
2. **`fd_write` (stdio) sempre consentito — preopen-aware il resto.**
   Il draft includeva `fd_write` sotto preopens; realizzato:
   `fd_write`/`fd_fdstat_get`/`fd_close`/`fd_seek` sono SEMPRE consentiti
   perché il ctx li mappa su pipes memoria catturate (stdout/stderr) e
   stdin vuoto — NON toccano il filesystem (invariante n.4). Il file I/O
   reale (`path_open`, `fd_read`, readdir…) resta gateato su preopens.
3. **Nessun standalone benchmark "overhead ≤ 20%" (test 9).** Il layer
   riusa il medesimo profilo (fuel/epoch/pooling/cache di core) e non aggiunge
   lavoro per-run: il benchmark del core (`cargo bench -p wasmbox-core`) resta
   il metro; un bench dedicato layer-on/off è posticipato (v0.9).

### Storia: cosa è stato aggiunto rispetto alla v0.8 draft

- `WasiEngine::new` (strict) / `new_permissive`;
- cache `.cwasm` identica al core (hash DefaultHasher, write atomico
  tmp→rename, ricompilazione su corruzione);
- test di accettazione della spec (denied import, guest ABI puro,
  random opt-in, stdout captured, fuel con WASI, ask round-trip) — 7/7
  PASS: `cargo test -p wasmbox-wasi`. I test con preopen file reale
  (spec test 2/3/4) restano fuori scope v0.8 (vedi §9).
