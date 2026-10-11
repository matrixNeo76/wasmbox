---
type: Reference
title: "wasmbox — Analisi Comparativa, Threat Model e Posizionamento Architetturale (v0.2 consolidated)"
description: "Confronto fact-checked con le principali sandbox (Firecracker/E2B, gVisor, Sandlock, Extism, RLBox, Eryx, sandboxd), threat model Spectre/memoria e roadmap di consolidamento. Verifica: 2026-10-11."
resource: "docs/comparative-analysis.md"
tags: ["comparison", "threat-model", "sandbox", "wasm", "posizionamento", "okf"]
updated: "2026-10-11"
---

# Analisi Comparativa, Threat Model e Posizionamento Architetturale (`docs/comparative-analysis.md`, OKF v0.2)

**Repository**: <https://github.com/matrixNeo76/wasmbox> · **Autore / Maintainer**: matrixNeo76
**Document Type**: Architectural Review, Comparative Analysis & Hardening Blueprint
**Versione**: 0.2 (Consolidated, Fact-Checked & Refined) · **Data di riferimento**: October 2026
**Status**: Approvato per il consolidamento tecnico e l'implementazione
**Fact-check**: 2026-10-11, fonti web (advisory Wasmtime, repo ufficiali Firecracker / E2B / gVisor / Extism / RLBox / Eryx / sandboxd / Sandlock).

---

## 1. Executive Summary & Verdetto di Validazione

La tesi architetturale di wasmbox è confermata e validata: il progetto si
posiziona con chiarezza nel dominio del **confinamento di pure compute
in-process con approccio deny-by-default**: non entra in competizione diretta
con le piattaforme full-system (Firecracker, E2B) né con i kernel applicativi
(gVisor), ma massimizza efficienza e isolamento formale nell'esecuzione di
algoritmi e funzioni non fidate.

Il consolidamento alla v0.2 formalizza i dettagli operativi emersi dalla
revisione dello stato dell'arte (2025–2026) e dalla lettura dello **stato
reale del codice** (v0.8, `wasmbox-core` + layer WASI separato):

- **Trade-off tra Isolamento In-Process e Canali Laterali (Spectre/Meltdown)**:
  l'isolamento software (SFI) della memoria lineare Wasm protegge l'integrità
  logica dello spazio di indirizzamento, ma condividere il processo host
  espone a speculazioni microarchitetturali se il guest esegue codice ostile
  mirato (§5, Criticità 2).

- **Validazione della memoria e protocollo `env::ask` (caso studio
  CVE-2026-35195)**: il passaggio di buffer raw (`req_ptr, req_len`) → `i64`
  è ad alte prestazioni, ma richiede validazione rigorosa. **Riscontro dal
  codice**: questa infrastruttura è **già implementata** in `wasmbox-core`
  — allocatore cooperativo `guest_alloc`/`guest_free` + bounds check
  esplicito (`checked_add` + confronto con `data.len()`) su *ogni* slice
  attraversata (input, richiesta `ask`, risposta `ask`, output), con
  `guest_alloc = 0` gestito come `GuestOutOfMemory` (mai deref a offset 0).
  Il rischio descritto dalla CVE è quindi **chiuso per design** in wasmbox;
  il residuo è l'obbligo documentale di §7 (spec ABI endianness/layout).

- **Ecosistema allargato**: oltre alle sandbox a processo OS-level
  (Sandlock: Landlock + seccomp) e ai wrapper opinati (sandboxd), il confronto include Extism (framework universale di plugin Wasm con PDK poliglotta) e
  RLBox (confinamento fine-grained di librerie C/C++ via WebAssembly/SFI in
  ambienti critici come Mozilla Firefox). Eryx estende il dominio agli
  interpreti dinamici (CPython in WASI/Wasmtime).

- **Sincronia del dispatcher host**: la gestione delle invocazioni verso
  l'host deve distinguere carichi CPU-bound e I/O-bound per non saturare il
  thread pool dell'host runtime (§5, Criticità 4).

## 2. Validazione Puntuale delle Affermazioni

| Affermazione / Componente | Valutazione | Note dettagliate & riscontri |
|---|---|---|
| "Zero fiducia nel guest (niente FS, niente rete)" | **Corretta** | Il "deny-by-default" è strutturale, non dichiarativo: il guest è di fatto un automa a stati finiti e la superficie d'attacco del sistema operativo si riduce radicalmente. Riscontro dal codice: `WasiCapabilities::default()` = **zero** capability (zero preopen, zero clock/random); ogni import `wasi_snapshot_preview1` non concesso viene verificato *al build* dell'engine (strict) e il modulo non istanzia — più severo di uno stub errno a runtime. *(Precisazione post-v0.8: il layer WASI opt-in trasforma il claim da "non c'è WASI" a "NESSUN WASI a meno che l'host non dichiari esplicitamente le capability"; il default resta deny-by-default.)* |
| "Controllo totale: Fuel + Epoch + StoreLimits" | **Corretta (pattern canonico)** | Config canonica per l'esecuzione di codice non fidato: fuel (metering deterministico delle istruzioni) + epoch interruption (wall-clock, un solo thread ticker per engine — mai thread-per-run) + `StoreLimits` per il tetto di pagine lineari. Riscontro dal codice: i tre limiti sono compositi nel profilo del core e nel layer WASI (identici per invariante n.2 di wasi-plan). |
| "Comunicazione opaca `env::ask`" | **Corretta, con requisiti di hardening già implementati** | L'host mantiene il monopolio semantico. **Riscontro dal codice**: bounds-checking esplicito su ogni attraversamento (in/out) e protocollo di allocazione cooperativo (`guest_alloc`/`guest_free`) L'host non scrive MAI in offset arbitrari: alloca via il guest, valida ptr+len, restituisce il packing. |
| "Firecracker / E2B / gVisor come MicroVM" | **Corretta con distinzione VMM vs Platform vs App kernel** | Firecracker è un VMM minimalista KVM; E2B è una piattaforma di orchestrazione sandbox per agenti AI che usa Firecracker con **ripristino da snapshot** di microVM pre-avviate (l'implementazione aperta carica la memoria lazy — detto "pool pre-riscaldato" è una semplificazione); gVisor opera come kernel applicativo (*application kernel* in userspace, `runsc`). |
| "Sandlock come OS-level sandbox (~5 ms)" | **Corretta + dettagli, cifra dichiarata dal progetto** | Sfrutta primitive Linux unprivileged: **Landlock ABI 6+** (regole FS/rete, introdotta dal kernel 6.12), **seccomp-bpf** (filtro syscall) e **seccomp user notification** (`SECCOMP_RET_USER_NOTIF`) per emulare o intercettare syscall sicure. Il README del progetto indica ~5 ms di startup; il benchmark pubblicato mostra ~2 ms bare-metal vs ~7 ms con Sandlock su `/bin/echo`, cioè ~5 ms di overhead. **Nota**: cifra del progetto, non benchmark indipendente. |
| "Eryx per Python non fidato" | **Corretta, con percorsi distinti** | Sandbox Python basata su Wasmtime con CPython (3.14) compilato per WASI: AOT/precompilazione + SandboxPool con pre-warming. Numeri dichiarati dal progetto (non indipendenti): ~16 ms per creazione con runtime precompilato (vs ~650 ms senza precompilazione), 1–5 ms nella creazione rapida con runtime pre-inizializzato. **Footprint 15–30 MB per interprete: NON verificato dalle fonti** (le pagine dichiarano dimensioni di artefatti: ~47 MB runtime Wasm, ~52 MB precompilato; RSS per istanza non documentato). |
| "sandboxd come layer opinato sopra Wasmtime" | **Corretta** | Crate + CLI Rust che aggiungono confine deny-by-default sopra Wasmtime: nessun import, WASI, filesystem o rete autorizzati per default; fuel/tempo/memoria come limiti. Esempio dato dichiarato dal progetto: ~10.6 ms per invocazione CLI a freddo su Apple M3 Pro (include avvio processo + compilazione modulo, non l'overhead isolato della libreria). |

## 3. Tassonomia delle Sandbox (Stato dell'Arte 2025–2026)

Le architetture di sandboxing si dividono in tre domini separati per confine
d'isolamento, velocità e costo di memoria:

+-----------------------------------------------------------------------------------------+
| LIVELLO 1: MICROVM / HARDWARE-ASSISTED (Firecracker, Cloud Hypervisor, E2B, gVisor*)     |
| -> Isolamento: KVM / Hardware Virtualization (kernel Linux guest indipendente)           |
|    (*gVisor: kernel applicativo in userspace, eseguibile sia con Sentry come kernel     |
|      applicativo sia con KVM come piattaforma - non è MicroVM pura)                      |
| -> VMM init: ~8 CPU ms (~12ms wall-clock, range 6–60ms) | Snapshot restore: < 5ms        |
| -> Full kernel boot (user-space /sbin/init): <= 125ms                                    |
| -> Overhead VMM (threads): <= 5 MiB [dichiarato, SPECIFICATION]; RAM = memoria assegnata |
| -> Use-case: esecuzione di codice arbitrario multi-tenant, "container", script bash     |
+-----------------------------------------------------------------------------------------+
                                             ▲
                                             │
+-----------------------------------------------------------------------------------------+
| LIVELLO 2: OS-LEVEL SANDBOX (Sandlock, Bubblewrap, Google Sandbox2)                     |
| -> Isolamento: kernel primitives (Landlock ABI 6+, seccomp-bpf, SECCOMP_RET_USER_NOTIF) |
| -> Cold start: 3–5ms | Footprint memoria: trascurabile (< 2MB overhead)                  |
| -> Use-case: confinamento di processi nativi Linux, interpreti CLI, tool di sistema     |
+-----------------------------------------------------------------------------------------+
                                             ▲
                                             │
+-----------------------------------------------------------------------------------------+
| LIVELLO 3: IN-PROCESS WASM RUNTIMES (wasmbox, Extism, sandboxd, Eryx, RLBox*)            |
| -> Isolamento: Software Fault Isolation (SFI) / memoria lineare Wasm (Cranelift/JIT)     |
|    (*RLBox: confinamento fine-grained di librerie C/C++ dentro il processo host           |
|      via WebAssembly + wasm2c; non è un runtime sandbox completo)                        |
| -> Cold start: sub-millisecondo (< 0.5ms con moduli AOT / .cwasm **per il profilo        |
|    wasmbox misurato**: compile freddo ~1.36ms, cache-hit ~0.14ms)                        |
| -> Footprint memoria: < 1MB per istanza (wasmbox/sandboxd) | Eryx: RSS per istanza      |
|    NON documentato (artefatti runtime: ~47MB Wasm / ~52MB precompilato)                  |
| -> Use-case: pure compute, espressioni e tool per LLM, plugin ad altissimo throughput    |
+-----------------------------------------------------------------------------------------+

> Nota onesta: i confini tra i tre livelli sono netti nel *tipo* di confine
> (hardware / kernel / in-process), non nei numeri assoluti (che
> restano macchina- e configurazione-dipendenti).

## 4. Matrice Comparativa Consolidata

| Dimensione architetturale | wasmbox | Extism | sandboxd | Eryx | Sandlock | Firecracker / E2B |
|---|---|---|---|---|---|---|
| Dominio di isolamento | In-Process Wasm (SFI) | In-Process Wasm (SFI) | In-Process Wasm (SFI) | In-Process Wasm (SFI) (CPython WASI) | Processo OS (Landlock/seccomp Linux) | MicroVM (hardware KVM) |
| Runtime engine | Wasmtime (Rust puro) | Wasmtime / V8 / Wazero (SDK poliglotti) | Wasmtime | Wasmtime + CPython 3.14 WASI | Kernel Linux nativo | Rust VMM + guest Linux |
| Latenza avvio (cold start) | ~1.36 ms (`new` freddo), ~0.14 ms (cache-hit `.cwasm`) [misurato, cargo bench] ~0.2 µs per run con round-trip `ask` | ~1 ms (dichiarato) | ~10.6 ms per invocazione CLI fredda (dichiarato, include process checkout+compile) | ~16 ms (senza pool, dichiarato) · 1–5 ms (con runtime pre-inizializzato, dichiarato) | ~5 ms (dichiarato dal progetto; ~2→7 ms su /bin/echo) | VMM ~8 ms CPU (~12 ms wall) · boot ≤ 125 ms · E2B ~150 ms (caso dichiarato Manus, con snapshot+lazy restore) |
| Overhead memoria per istanza | < 1 MB (moduli piccoli, pooling) | 1–3 MB (dichiarato) | < 2 MB (dichiarato) | non documentato per istanza (artefatti ~47/52 MB) | trascurabile (< 2 MB) | ≤ 5 MiB overhead thread VMM [spec]; RAM = assegnazione guest (tipicamente centinaia MB) |
| Limiti risorse out-of-the-box | **Fuel + Epoch + `StoreLimits` + budget `ask`** (compositi, applicati dentro `new` + `run`) | Memory, timeouts | Fuel + memory | Timeouts + memory | Cgroups (se configurati) | vCPU pinning, cgroups, limiti RAM |
| Canale host–guest | Opaco: `env::ask(i32,i32)->i64` (bytes, budget+bounds-check) + allocatore cooperativo | Extism PDK (Plugin Memory Protocol) | Host functions tipizzate | Async callbacks JSON | IPC / syscall mediate (USER_NOTIF) | VirtIO/vsock / rete TCP |
| Supporto codice dinamico (Python/JS) | No (solo Wasm compilato; CPython-WASI sarebbe un guest a parte) | Tramite plugin precompilati | No | Sì (CPython nativo WASI) | Sì (tramite interprete OS) | Sì (qualsiasi binario o script) |
| Complessità architetturale | **Minima** (2 crate: core + layer WASI opt-in) | Media | Bassa | Media | Bassa (dipende da Linux ≥ 6.12) | Alta (richiede hypervisor e KVM) |

## 5. Analisi Critica e Punti di Attenzione per wasmbox

### Criticità 1: protocollo di memoria `env::ask` & lezione CVE-2026-35195

Nel design di wasmbox la comunicazione è canalizzata tramite una singola host
function opaca:

```
env::ask(req_ptr: i32, req_len: i32) -> i64     // packed (res_ptr << 32) | res_len
```

**Il rischio (lezione della CVE)**: se l'host scrive la risposta direttamente
nella memoria lineare del guest allocando arbitrariamente (o fidandosi
ciecamente di un offset o del puntatore restituito da un *realloc* del
guest), si incorre in corruzione dello heap interno del modulo.

**Contesto CVE-2026-35195 (Wasmtime, GHSA-394w-hwhg-8vgm)**: l'advisory
Bytecode Alliance/GitHub (aprile 2026, Moderate, CVSS 6.1) descrive una
vulnerabilità nella gestione delle stringhe del Component Model: Wasmtime
non verificava che il puntatore restituito da una funzione `realloc` del
guest fosse nei limiti della memoria lineare al momento del'invocazione; la
scrittura dei byte transcodificati poteva arrivare fino a 4 GiB dalla base —
con paging standard, SIGSEGV → DoS; in configurazioni con guard pages
ridotte o memoria custom, corruzione di memoria adiacente. Corretti in
Wasmtime 24.0.7 / 36.0.7 / 42.0.2 / 43.0.1.

**Stato in wasmbox (verificato dal codice, non da questa analisi)**: la
posizione del progetto è puntatore-validata per design:

1. l'host non "indovina" MAI dove scrivere: chiama l'export cooperativo
   `guest_alloc(len: i32) -> i32` del guest (tipicamente un bump allocator
   interno al modulo) e poi **valida** esplicitamente:
   - `ptr == 0` con `len > 0` ⇒ `GuestOutOfMemory` (mai deref a offset 0 —
     per contratto `guest_alloc(0)` restituisce un puntatore non-nullo
     ed è `guest_free(ptr, 0)` un no-op);
   - `ptr.checked_add(len)` con overflow ⇒ `MemoryAccess` tipizzato;
   - `ptr + len > memory.data().len()` ⇒ `MemoryAccess` tipizzato (fallback
     a trap → errore; niente scrittura).
2. lo stesso bounds check è applicato a *ogni* attraversamento: input
   host→guest, richiesta `ask` guest→host, risposta `ask` host→guest, output
   guest→host (`unpack_ptr_len` + to_vec());
3. la regola di packing è documentata come contratto ABI vincolante nel
   blueprint (`pack_ptr_len`/`unpack_ptr_len`: layout `(ptr << 32) | len` in
   un `u64` passato come `i64`).

**Residuo (action item)**: scrivere la specifica formale endianness/layout
dell'ABI `env::ask` (§7 roadmap) — la semantica è quella descritta, manca
solo il documento che la sancisca come standard per gli SDK guest in Rust,
C, Go e Zig.

### Criticità 2: isolamento SFI vs attacchi a canale laterale (Spectre/Meltdown)

La memoria lineare WebAssembly garantisce isolamento a livello logico (SFI).
Poiché il motore JIT (Cranelift) compila il bytecode in codice macchina
eseguito nello **stesso spazio di indirizzamento virtuale del processo
host**, un modulo malevolo con gadget di esecuzione speculativa (Spectre-v1,
Spectre-v2, Branch Target Injection) può tentare di dedurre contenuti della
memoria dell'host tramite timing attack sulla cache della CPU.

**Mitigazione architetturale per wasmbox (raccomandazione, non implementata)**:
mantenere la modalità di default in-process per velocità e carichi
fidati/semi-fidati, introducendo per contesti rigorosamente zero-trust una
modalità opzionale **out-of-process worker** (processo figlio minimale con
spazio di indirizzi dedicato), coerente con la strategia del layer WASI
("opt-in al rischio, non default").

### Criticità 3: posizionamento strategico rispetto a WASI 0.2 e Component Model

WASI 0.2 (Preview 2) e il Component Model sono lo standard consolidato per
l'interoperabilità poliglotta tipizzata via WIT. wasmbox compie una scelta
deliberata: **non impone WIT né Component Model**, espone un'ABI minima
grezza a bytes opachi (4 export + 1 import: v. blueprint §CONTRATTO ABI).

**Raccomandazione di posizionamento**: comunicarla esplicitamente non come
carenza tecnologica ma come **elemento distintivo — "Zero-Dependency ABI"**:
riduce i binari guest a una frazione di kilobyte ed evita l'overhead del
layer canonico di boxing/unboxing del Component Model. Da maturare: quando
l'ecosistema SDK si aprirà (C, Go), prevedere eventuali SDK che *generano*
codice Component-Model-compatibile che **wrappa** l'ABI grezza (non
sostituendola).

*Precisazione post-fact-check (2026-10-11)*: il layer `wasmbox-wasi` (v0.8)
usa **Preview 1 subset capability-gated**, non Preview 2; la spec wasi-plan
dichiara Preview 2 fuori scope v0.8. Questa coerenza va mantenuta nella
comunicazione.

### Criticità 4: sincronia del dispatcher host

Nel thread pool di un runtime asincrono (es. Tokio), se la host function
`env::ask` effettua operazioni bloccanti (I/O, DB, RPC), il worker thread
viene sequestrato per tutta l'operazione.

**Linee guida per wasmbox** (verificate nel codice): il crate core è
sincrono per costruzione — `run()` è sincrono (l'unico thread dedicato è
quello del ticker epoch, zero dipendenze async); l'handler
`ask` è chiamato nel chiamante (stesso thread dello Store). Quindi:

- per compiti **CPU-bound wasmbox** si raccomanda handler `ask` sincrono,
  puro, senza attese I/O;
- per deleghe I/O asincrone, l'host applicativo che integra wasmbox ha due
  opzioni: (a) eseguire i run su thread dedicati esterni all'event loop;
  (b) mantenere l'approccio sync e gestire il parallelismo di scheduling
  (l'opzione `async` di Wasmtime con fiber cooperative resta fuori scope
  v0.8 — il blueprint vieta thread-per-run nel crate, ma nulla impedisce al
  *chiamante* di parallelizzare cu questa API).

## 6. Roadmap di Consolidamento Architetturale (v0.2 → v1.0)

> Stato dei singoli item aggiornato al codice reale 2026-10-11:

+---------------------------------------------------------------------------------------+
| FASE 1: v0.8.x — Hardening del Protocollo di Memoria & ABI Documentata                 |
| [x] Allocatore cooperativo (export guest wasmbox_alloc / wasmbox_dealloc)              |
|     → `guest_alloc`/`guest_free` già richiesti dal blueprint e usati dall'host in      |
|       `ask_host_function`; un guest che non li esporta non istanzia (MissingExport).   |
| [x] Bounds checking esplicito e atomic su ogni slice host↔guest                        |
|     → `checked_add` + `data.len()` su input/richiesta/risposta/output; `ptr==0`        |
|       gestito come `GuestOutOfMemory`, mai deref a offset 0.                            |
| [ ] Documentazione formale dell'ABI `env::ask`: specifica endianness, packing, layout   |
|     → da fare: è il contratto di §5 Criticità 1 (residuo di consolidamento).           |
+---------------------------------------------------------------------------------------+
                                           │
                                           ▼
+---------------------------------------------------------------------------------------+
| FASE 2: v0.9.x — Suite di Benchmark Pubblica & Trasparenza Prestazionale               |
| [x] Benchmark di avvio a freddo con moduli precompilati AOT (.cwasm)                   |
|     → `benches/perf.rs`: freddo ~1.36 ms, cache-hit ~0.14 ms; job CI `benchmark`       |
|       pubblica l'artifact a ogni push su main (2026-10-11).                            |
| [ ] Benchmark round-trip `ask` a granularità crescente (64B, 4KB, 64KB, 1MB)           |
|     → parzialmente fatto (round-trip ~0.2 µs + overhead ~0.7 µs con payload "bench");  |
|       il sweep a granularità di payload è il residuo.                                   |
| [ ] Confronto diretto di latenza vs Wasmtime grezzo ed Extism                          |
|     → da fare; lo scenario-G attiva il confronto Extism (proposto per v0.9).           |
| [ ] Benchmark layer WASI layer-on/off (test 9 di wasi-plan §6, già differito a v0.9)   |
+---------------------------------------------------------------------------------------+
                                           │
                                           ▼
+---------------------------------------------------------------------------------------+
| FASE 3: v0.10.x — Difesa in Profondità & Gestione Concorrenza                          |
| [ ] Modalità facoltativa out-of-process worker vs canali laterali CPU (Spectre)        |
| [ ] Supporto esplicito per host function asincrone (integrazione Tokio/fiber).         |
|     Nota: blueprint vieta thread-per-run nel crate; l'async resta un'opzione           |
|     dell'applicativo host, non del crate (o feature opt-in discussa in blueprint).     |
| [ ] Config granulare del fuel refill per esecuzioni a task interconnessi               |
+---------------------------------------------------------------------------------------+
                                           │
                                           ▼
+---------------------------------------------------------------------------------------+
| FASE 4: v1.0 — Stabilizzazione API & Certificazione di Produzione                      |
| [ ] Stabilizzazione dell'ABI C-compatibile per guest in Rust, C, Go e Zig              |
| [ ] Audit formale di sicurezza sul codice unsafe del crate core (2 blocchi SAFETY-     |
|     documentati: transmute fat pointer + deref handler; CodeQL già attivo).           |
| [ ] Rilascio di SDK guest minimalisti (Crate Rust zero-alloc, header C)                |
+---------------------------------------------------------------------------------------+

## 7. Conclusioni & Checklist Operativa

wasmbox dimostra un'architettura essenziale, coerente e focalizzata. Il suo
vantaggio competitivo risiede nella totale assenza di complessità superflua
per scenari di calcolo puro.

Checklist immediata per il maintainer (stato al 2026-10-11):

- [x] **Hardening memoria**: verifica preventiva di ogni puntatore prima di
  ogni dereferenziazione/ scrittura dall'host verso la memoria lineare —
  **fatto nel codice** (checked_add + data.len() + resp_ptr==0 guard).
- [x] **Standardizzare alloc**: `guest_alloc`/`guest_free` sono parte del
  contratto ABI; il guest di riferimento è `examples/guest-echo` (Rust) e il
  modulo nel benchmark; per l'header C → FASE 4 (SDK).
- [ ] **Comunicazione architetturale**: posizionare ufficialmente wasmbox
  come la scelta ottimale per *"Safe In-Process Pure Compute & LLM Tool
  Evaluation"* — README ha già la sezione "Perché è la scelta giusta per
  tool generati da LLM"; manca una pagina di posizionamento che citi
  esplicitamente il confronto (link a questo documento).
- [ ] **Spec ABI formale** (`env::ask` endianness/layout) — FASE 1 residuo.

## Fonti (fact-check 2026-10-11)

- CVE-2026-35195 / GHSA-394w-hwhg-8vgm — advisory Wasmtime sul Component
  Model (realloc non verificato): <https://github.com/bytecodealliance/wasmtime/security/advisories/GHSA-394w-hwhg-8vgm>,
  sintesi Bytecode Alliance <https://bytecodealliance.org/articles/wasmtime-security-advisories>,
  RustSec <https://rustsec.org/advisories/RUSTSEC-2026-0091.html>.
- Firecracker (VMM specification, avvio/memory): <https://github.com/firecracker-microvm/firecracker/blob/main/SPECIFICATION.md>.
- E2B (runtime Firecracker, snapshot restore, ~150 ms caso Manus):
  <https://github.com/e2b-dev/runtime>, <https://e2b.dev/customers/how-manus-uses-e2b-to-provide-agents-with-virtual-computers>.
- gVisor (application kernel): <https://github.com/google/gvisor>.
- Extism (plugin Wasm, PDK): <https://extism.org/docs/quickstart/plugin-quickstart/>.
- RLBox (fine-grained sandboxing di librerie C in Firefox):
  <https://rlbox.dev/>, <https://hacks.mozilla.org/2021/12/webassembly-and-back-again-fine-grained-sandboxing-in-firefox-95/>.
- Eryx (CPython in WASI su Wasmtime; AOT + SandboxPool): <https://github.com/eryx-org/eryx>,
  <https://pypi.org/project/pyeryx/>.
- sandboxd (wrapper Wasmtime deny-by-default): <https://github.com/sarmakska/sandboxd>,
  <https://www.sarmalinux.com/products/sandboxd>.
- Sandlock (Landlock ABI 6 + seccomp user notification, ~5 ms):
  <https://github.com/multikernel/sandlock>, <https://sandlock.io/>,
  <https://multikernel.io/2026/03/14/introducing-sandlock/>.

> **Nota di onestà**: tre progetti (Eryx, sandboxd, Sandlock) sono emersi
> nel 2025–2026 e i loro numeri sono dichiarati dai rispettivi progetti, non
> riprodotti da questo workspace; dove è stato possibile, la matrice indica
> "misurato" (solo per i dati wasmbox su questa macchina) vs "dichiarato".
