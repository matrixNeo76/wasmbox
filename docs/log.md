---
type: Log
title: "log — aggiornamenti documentazione wasmbox"
description: "Storia datata degli aggiornamenti del bundle di documentazione (solo eventi rilevanti per i documenti)."
okf_version: "0.2"
updated: "2026-10-11"
---

# Log — modifiche alla documentazione

## 2026-10-11 — piano agent-loop (docs/agent-loop-plan.md, v0.9 da approvare)

- Richiesta utente: valutare come Manus AI, Claude (Code), Codex e i
  principali framework SOTA usano coding e sandbox per risparmiare token
  nelle chat agentiche, cercando documenti aggiornati all'11-10-2026, ed
  elaborare una nuova spec OKF v0.2 per creare questa funzionalità nel
  repo in modo modulare, così che un LLM possa testare subito wasmbox
  senza perdere l'unicità del core.
- **Fact-check web (2026-10-11, due ricerche parallele)** su fonti
  primarie: Manus (context engineering: prefix stabile, tool masking,
  filesystem come contesto esterno), Anthropic (effective context
  engineering; writing tools; **code execution with MCP** — 150k→2k
  token), Claude Code docs (compaction, subagent, Bash output cap ~30k,
  CLAUDE.md ≤200 righe), OpenAI Codex (best practices, prompting guide:
  tool cap ~10k token con troncamento middle, apply_patch, config
  `/compact` + `model_context_window`), MCP spec 2025-06-18 (stdio,
  tools, structuredContent), framework sandbox 2026 (E2B, Daytona,
  Modal, LangChain Deep Agents, Wassette/`MCP↔Wasmtime`, Wasmer SDK
  ~0.1 ms claim locale).
- **Schema condiviso estratto**: la sandbox serve per ESEGUIRE e
  restituire un envelope piccolo e decisivo; mai riversare transcript;
  cap su output e errori; runtime persistente/warm-reuse; un solo tool
  per la superficie agente.
- **Nuova spec `docs/agent-loop-plan.md`** (OKF v0.2, `type:
  Specification`, stato DA APPROVARE — nessun codice prima della
  conferma utente): crate nuovo `crates/wasmbox-mcp` (STM-mirror della
  disciplina di wasmbox-http: JSON a mano, solo std + core), un solo
  tool `wasmbox_run`, envelope capato (`output` 4 KiB default con
  head+tail, `message` 256 B, `ask_calls`, `duration_ms`,
  `output_truncated`, campi in ordine stabile per KV-cache), server MCP
  stdio persistente (3 metodi: initialize/tools/list/tools/call,
  protocollo `2025-06-18`), 6 test round-trip + guard anti-bloat
  envelope, core intatto (Invariante 0).
- `index.md` aggiornato (riga concept + settore).
- **Revisione post-note dell'utente (2 obiezioni, entrambe fondate)**:
  1. *"i framework citati non usano MCP ma skills + CLI + sandbox"* →
     verificato sui vendor docs: Claude Code, Codex CLI, Cursor,
     Windsurf hanno tutti shell+sandbox proprie e usano SKILL per
     guidare la CLI; MCP è documentato come integrazione strutturata
     AGGIUNTIVA dagli host senza shell. Spec corretta: la via primaria
     resta **skills + CLI + envelope JSON** (già presente nel repo:
     SKILL.md + wasmbox-cli --json); MCP stdio è il canale aggiuntivo
     per host desk-side/no-shell; titolo e gerarchia aggiornati.
  2. *"hai valutato A2A, SSE e altri protocolli per rafforzare il
     core?"* → nuova sezione §2.5 con verdict per protocollo: A2A v1.0
     (agent↔agent, Linux Foundation, 150+ org, binding
     JSON-RPC/gRPC/HTTP — NON un'API di tool/sandbox), SSE (transport
     deprecato della vecchia MCP HTTP+SSE; resta come stream opzionale
     in Streamable HTTP), ACP (assorbito in A2A), AGNTCY/LMOS
     (infrastruttura federata, LMOS non W3C), MCP Streamable HTTP
     (unico protocollo candidabile per remoto in v1.0). Fact-check
     aggiuntivo: la spec MCP corrente è **2026-07-28** (la versione
     2025-06-18 citata in prima stesura era superata) — correzione
     applicata anche al protocolVersion dichiarato dal server.

## 2026-10-11 — limiti MISURATI nel benchmark pubblico (sweep payload + rifiuto enforced)

- Richiesta utente: rimarcare con dati riproducibili (bench) i limiti del
  progetto dove i competitor non possono farlo, e evidenziarlo in README
  e documenti.
- **`benches/perf.rs` esteso** (solo std, nessuna nuova dipendenza):
  - sweep round-trip `ask` a payload 64B/4KB/64KB/1MB (n=500): ~10 /
    ~12 / ~48 / ~580 µs; costo marginale ~0.55 ns/byte;
  - **limite ENFORCED misurato**: payload 1 MiB+1 > tetto 1 MiB → rifiuto
    con `SandboxError::PayloadTooLarge` tipizzato in ~535 ns, verificato
    realmente nel bench (panico se diverso), PRIMA di toccare il guest.
  - nuovo modulo `ASK_BIG_WAT` (16 MiB lineari, heap da `memory.size`,
    bump resetato a ogni `guest_run`): serve perché il modulo piccolo
    del bench principale ha un bump allocator con tetto 64KB che
    saturerebbe a payload grandi (bug del primo tentativo, fixato).
- **README**: sezione "Benchmark pubblici — i limiti MISURATI, non
  dichiarati" con la differenziazione esplicita: i competitor
  (Eryx, sandboxd, Sandlock) pubblicano cifre dichiarate non
  riproducibili; i nostri escono da `cargo bench` (una riga) e dalla CI
  (artifact a ogni push). Tabella aggiornata al 2026-10-11.
- `comparative-analysis.md` FASE 2: sweep 64B/4KB/64KB/1MB e limite
  ENFORCED segnati [x]; resta il confronto diretto Extism/Wasmtime
  grezzo e il layer-on/off WASI (v0.9). ROADMAP aggiornata.
- Importante per l'onestà del claim: la macchina host è una VM shared,
  i numeri assoluti fluttuano (overhead `ask` misurato tra ~0.6 e ~1.8
  µs in run consecutive); la citazione corretta è con n e data, come
  fa la tabella (e per questo l'artifact CI è datato per commit).

## 2026-10-11 — nuova analisi comparativa fact-checked (docs/comparative-analysis.md, v0.2)

- Richiesta utente: rielaborare il documento OKF di comparazione con le
  altre principali sandbox, salvarlo aggiornato nel percorso corretto e
  poi aggiornare la ROADMAP di conseguenza.
- **Fact-check web (2026-10-11)** eseguito prima del consolidamento:
  - CVE-2026-35195 → confermata come
    [GHSA-394w-hwhg-8vgm](https://github.com/bytecodealliance/wasmtime/security/advisories/GHSA-394w-hwhg-8vgm)
    (advisory Bytecode Alliance, aprile 2026, CVSS 6.1, Moderate; fixed in
    24.0.7/36.0.7/42.0.2/43.0.1; RustSec RUSTSEC-2026-0091);
  - Firecracker (spec VMM: ≤8ms CPU init, ≤125ms boot, ≤5MiB overhead
    threads), E2B (snapshot restore + lazy memory, ~150ms caso Manus),
    gVisor (application kernel), Extism (PDK poliglotta), RLBox
    (fine-grained in-process) → confermati con precisazioni;
  - Eryx, sandboxd, Sandlock → esistono; numeri marcati "dichiarati dal
    progetto"; il claim "15–30 MB per interprete Eryx" NON è stato
    confermato dalle fonti (esse dichiarano solo dimensioni di
    artefatti ~47/52 MB, non RSS per istanza).
- **Consolidamento contro il codice reale**: il documento rivendica che
  l'"hardening" richiesto dalla Criticità 1 (allocatore cooperativo +
  bounds check su ogni attraversamento host↔guest) è GIÀ IMPLEMENTATO in
  `wasmbox-core` (verificato su `src/engine.rs`: `guest_alloc`/`guest_free`
  + `checked_add` + `data.len()` + guard `ptr==0` → `GuestOutOfMemory`),
  quindi FASE 1 della roadmap di consolidamento ha 2/3 item chiusi; resta
  aperta la spec formale ABI endianness/layout.
- Percorso: `docs/comparative-analysis.md` (OKF v0.2, `type: Reference`,
  frontmatter completo). Sezione "Fonti" con tutti gli URL letti e nota
  di onestà su misurato-vs-dichiarato.
- `index.md` aggiornato (riga concept + ordine di lettura 4c).

## 2026-10-11 — WASI opt-in IMPLEMENTATO (crates/wasmbox-wasi, v0.8)

- Richiesta utente: conferma del piano WASI ("confermo puoi procedere").
- **Nuovo crate `crates/wasmbox-wasi`** (membro workspace): layer WASI
  Preview 1 opt-in sopra wasmbox-core — il core resta intatto (nessuna
  dipendenza da wasmtime-wasi lì; divieto blueprint n.1 verificato con
  grep).
- API: `WasiCapabilities` (default = zero capability; `preopen_readonly`/
  `preopen_readwrite`, `allow_clock`, `allow_random` opt-in espliciti),
  `WasiEngine::new` (strict: il guest che importa syscall non in
  allow-list NON istanzia — `WasiError::Denied`) e `new_permissive`;
  `run(input, handler) -> (output, stdout_captured)` con fuel/epoch/
  memoria/cache `.cwasm` identici al core. Stdout/stderr catturate con
  `MemoryOutputPipe` (mai scritte sul processo host); stdin sempre vuoto.
- Deps nuova SOLO nel crate wasi: `wasmtime-wasi 49 (feature p1)` +
  `thiserror` (col warning della nota dipendenze blueprint resta il
  core immutato).
- Test accettazione `tests/wasi_plan_test.rs` — **7/7 PASS**
  (`cargo test -p wasmbox-wasi`): denied strict (random con default),
  guest ABI puro senza WASI, random opt-in ok, stdout captured reale
  (`fd_write` → pipe), clock consentito + fuel bomb → `FuelExhausted`,
  `ask` round-trip dentro un guest WASI (2 ask registrate), diagnosi
  delle denied import.
- Docs: `wasi-plan.md` aggiornato (stato IMPLEMENTATO + §10 deviazioni
  oneste: strict al build al posto di errno; `fd_write` stdio sempre
  consentito; benchmark layer-on/off posticipato a v0.9), README (riga
  estensioni), `index.md`.
- **Deviazioni 2/3/4 della spec (preopen read test, benchmark) restano
  out-of-scope v0.8**, dichiarate in wasi-plan.md §9/§10.

## 2026-10-11 — posizionamento LLM, spec WASI opt-in, benchmark in CI

- Richiesta utente (2026-10-11): valutare e procedere su tre proposte: (1)
  benchmark pubblici via CI, (2) layer WASI opt-in, (3) evidenziare il
  posizionamento per tool generati da LLM. Ordine eseguito: (3) → (2) → (1).
- **(3) Posizionamento LLM**: nuova sezione README "Perché è la scelta
  giusta per tool generati da LLM" — niente WASI di default (il guest non
  può *aprire* nulla), tool-bus nell'host via `HostHandler::ask`, limiti
  espresso (fuel/epoch/memoria/budget), errori tipizzati con exit code
  deterministici, sei vie d'uso provate (CLI/libreria/HTTP/FFI + scenari
  F e S3 per agenti LLM).
- **(2) Spec WASI opt-in**: creato `docs/wasi-plan.md` (OKF v0.2, `type:
  Specification`, v0.8 DA APPROVARE — nessun codice senza conferma):
  nuovo crate `crates/wasmbox-wasi` SEPARATO dal core (divieto blueprint
  n.1 intatto: grep `wasmtime-wasi` su `src/` del core = 0), capability
  model deny-by-default (preopen readonly, stdio captured, clock/random
  opt-in; MAI sock/poll/environ/argv), subset Preview 1, API draft
  `WasiEngine` con `run(input, handler)` + stdout captured, 10 test di
  accettazione (inclusi path traversal e fuel/epoch invariati), rischi
  dichiarati. `index.md` aggiornato.
- **(1) Benchmark pubblici**: nuovo job CI `benchmark` in
  `.github/workflows/ci.yml` — su ogni push a `main` esegue
  `cargo bench -p wasmbox-core -- --nocapture`, genera
  `bench-report/README.md` con metadati ambiente (commit, runner, data) e
  lo pubblica come artifact `bench-report` scaricabile. Il job NON è gate
  (la baseline resta il job principale). README: sezione "Benchmark
  pubblici" con tabella dei valori di riferimento, caveat
  macchina-dipendenti e istruzioni per scaricare l'artifact.
- Nessuna modifica al codice Rust: baseline invariata.

## 2026-10-10 — nuova guida scenari d'integrazione (docs/scenarios.md)

- Richiesta utente: documentazione approfondita su come implementare gli
  scenari più diffusi agganciati al core via `ask` (primo spunto: backend
  FastAPI), comprendibile da LLM e umani.
- Creato `docs/scenarios.md` (OKF v0.2, `type: Guide`) con **6 ricette
  complete**: S1 FastAPI ask-proxy (handler Rust che inoltra `ask` al
  backend Python — architettura, codice Rust+Python, latency, sicurezza),
  S2 gRPC bridge (tonic + caveat tokio current-thread), S3 agente LLM
  tool-bus (flusso invertito vs scenario F, limits consigliati), S4
  CLI/subprocess con contratto JSON (già implementato), S5 serverless
  (tabella per provider, caveat cold-start cache), S6 FFI in-process
  (ctypes, vincoli GIL/thread). Matrice di decisione iniziale per scegliere
  lo scenario + tabella riepilogo + checklist produzione comune.
- `index.md` aggiornato (riga concept + ordine di lettura 4b).
- Regola di coerenza rispettata: ogni scenario cambia SOLO protocollo
  opaco e logica handler; la sandbox resta identica (fuel/memory/timeout/
  budget `ask`).

## 2026-10-10 — estensioni v0.7 implementate (M1 + scenari D/E/F)

- Richiesta utente: eseguire il piano `docs/extension-plan.md` nell'ordine
  indicato ("tutto ok").
- **M1** (`examples/scenario-limit-runner`): run fuel-only deterministica
  (timeout disattivato) — fuel-bomb → **exit 3 su 3 run consecutive**;
  `scenario-c.sh` aggiornato con case `fuel-bomb-fuel-only` (4/4 PASS).
- **Scenario D** (`crates/wasmbox-http`): endpoint HTTP std (`TcpListener`,
  thread per connessione, JSON+base64 scritti a mano, zerodipendenze oltre
  al core). Rotta unica `POST /run` + `GET /healthz`; errori sandbox con lo
  STESSO vocabolario della CLI. 10/10 unit test; `scenario-d.sh` 4/4 PASS
  (echo 200, bad_request 400, invalid_wasm, fuel deterministico via
  `limits.max_fuel` + `epoch_timeout_ms:null`). Trappola C efixata:
  `base64::decode` con filtro silenzioso accettava bytes estranei →
  riscritto con validazione esplicita; json numeri negativi via u128.
- **Scenario E** (`crates/wasmbox-ffi`): C-ABI stabile
  (`cdylib+staticlib`, zero dipendenze): `new/free/run/buffer_free/`
  `last_error(_code)`; status 0/1/2/3 + exit code stile CLI. 3/3 test Rust
  (round-trip echo reale via guest wasm32 compilato, argomenti illegali →
  status 1, wasm invalido → NULL). `scenario-e.sh` via python3 **stdlib**
  (ctypes): 5/5 PASS — prova che un host non-Rust può usare wasmbox.
- **Scenario F** (feature `llm` opt-in su `scenario-tool-handler`, `ureq`
  optional con solo `rustls`): tool `llm:<prompt>` via OpenRouter; la rete
  sta SOLO nell'host handler; senza `OPENROUTER_API_KEY` → exit 7 tipizzato
  (`scenario-f.sh` PASS offline; chiamata reale SKIP finché chiave assente).
- Docs: `integration.md` §4 riscritta (HTTP/FFI/LLM **fatti**, con prove),
  tabella scenari D/E/F, `index.md` (extension-plan + nuovi crate), questo
  log; ROADMAP aggiornata.
- Divieti rispettati: niente serde/serde_json (JSON a mano in HTTP e LLM),
  un solo nuovo meccanismo di rete (host handler, opt-in), FFI senza
  thread e senza puntatori guest uscenti dal crate.

## 2026-10-10 — 3 scenari eseguibili (integration.md portati in pratica)

- Richiesta utente: creare gli esempi dei 3 scenari della guida.
- **Scenario A** (`examples/scenario-cli-pipeline` + `scripts/scenarios/scenario-a.sh`):
  pipeline di fiducia — 4 case (normale/vuoto/100KB/non-UTF8) via
  `wasmbox-cli --json`, verdetto TRUST/REJECT, registro per SHA-256.
  Testato: TRUST + registrazione.
- **Scenario B** (`examples/scenario-tool-handler` con `guest/` dedicato):
  protocollo `tool:<nome>:<arg>` → `tool_result:…` con `HostHandler` reale,
  metriche post-run, errore host → exit 7. Testato: soma=42, reverse, len,
  boom→exit 7.
- **Scenario C** (`examples/scenario-hostile-guest` + runner): 3 bombe WAT
  (fuel/oom/ask-flood) compilate via `crates/wat-compile-scenarios` (crate
  `wat`, dev-dep già autorizzata). Testato 4 volte: fuel→exit 3 o 4
  (non deterministico: entrambi limiti validi, il runner li accetta),
  OOM→5, ask-flood→6. Zero crash.
- `docs/integration.md` §5 "Scenari provabili", `index.md` aggiornati;
  baseline completa verde (check/test 22/22/clippy/fmt).
- Workspace: 2 nuovi membri (scenario-tool-handler + guest,
  wat-compile-scenarios).

## 2026-10-10 — nuova guida d'integrazione (docs/integration.md)

- Richiesta utente: documento approfondito su come implementare wasmbox in
  altri sistemi, applicativi e agenti — il buco era confermato dalla
  ricerca (SKILL.md copre solo la via CLI e dichiara il proprio limite;
  host-run è un esempio senza spiegazione).
- Creato `docs/integration.md` (OKF v0.2, `type: Guide`): le tre vie
  (binaria/libreria/estensioni), mappa exit code CLI con politica per
  agente, `SandboxConfig` campo per campo con default reali dal sorgente,
  mappa completa `SandboxError` con politica host, protocollo consigliato
  sopra `ask` (request/response tipizzate lato host), template guest
  conforme + validazione con input ostile, checklist finale.
- `index.md` aggiornato (riga concept + ordine di lettura); il grafo nel
  preview raccoglierà i nuovi nodi/links automaticamente al prossimo
  render (scan on-the-fly dei link).

## 2026-10-10 — mappa dei concetti OKF come grafo nel preview

- Richiesta utente: indice OKF + grafo (vis.js/d3). Deciso col cliente:
  grafo nel preview report (`scripts/render_report.py`), generato al volo
  scansionando i link markdown reali — zero drift con il repo; niente
  `docs/graph.html` permanente (coerente con la decisione «no graphify»
  del 2026-10-08) ma la funzione `build_okf_graph` è riusabile per un
  evento standalone futuro.
- Verificato: 8 nodi (5 doc bundle + README/AGENTS/SKILL), 18 archi reali
  deduplicati; vis-network via CDN; JS inline e JSON grafo validati;
  preview gratuitato `freebuff-preview restart` → pagina servita con
  sezione grafo (`id="okf-graph"`) + screenshot UI (HTTP 200).

## 2026-10-10 — matrice multipiattaforma VERIFICATA in CI (tag v0.5.1)

- Tag `v0.5.1` spinto → workflow Release: **tutti e 4 i job verdi**
  (ubuntu-latest, macos-15, macos-15-intel, windows-2025).
- Release `v0.5.1` con gli **8 asset attesi**: 4 archivi con target-triple
  (3× tar.gz + 1× .zip su Windows) + 4 file .sha256.
- ROADMAP §2.0 aggiornato col riscontro reale: la matrice diventa la release
  standard per ogni tag successivo.

## 2026-10-10 — spec + piano per la portabilità multipiattaforma (v0.6)

- Richiesta utente: usare wasmbox in TUTTI gli ambienti, non solo Linux.
- `blueprint.md` → **v0.6**: nuova sezione "PORTABILITÀ MULTIPIATTAFORMA"
  con la matrice di release (4 runner/target triple: linux-gnu,
  aarch64-apple-darwin, x86_64-apple-darwin, x86_64-pc-windows-msvc), le
  regole del workflow a matrice, i rischi documentati e l'ordine di
  implementazione (incluso il tag di verifica `v0.5.1`).
- Codice Rust dichiarato INVARIATO (std-puro): cambia solo la superficie di
  rilascio (`.github/workflows/release.yml`, packaging `.tar.gz`/`.zip`,
  nomi asset con target-triple).
- ROADMAP §2.0 aggiornato: residuo "runner multipli" aggiornato con lo stato
  della spec e del piano.

## 2026-10-09 — release GitHub con binari scaricabili

- Nuovo workflow `.github/workflows/release.yml`: su tag `vX.Y.Z` builda
  release (wasmbox-cli, wasmbox-ui, guest_echo.wasm), esegue baseline +
  smoke test (CLI --json, screenshot BMP) e pubblica la release con tar.gz
  + SHA-256. PRIMA non esistevano release su GitHub (verificato: 0).
- README: sezione "Release GitHub" con istruzioni tag e uso dei binari.
  Limitazione dichiarata: solo linux x86_64 per ora.

## 2026-10-09 — interfacce di fruizione (CLI + skill + UI)

- Blueprint → **v0.5**: nuova sezione "INTERFACCE DI FRUIZIONE" con la spec
  di `wasmbox-cli` (exit code 0/2..10, `--json`), `wasmbox-ui`
  (screenshot BMP headless, senza systemfonts) e la skill agenti.
- ROADMAP §3 punto 10: cronologia della sessione con prove.
- README: quickstart CLI/UI, struttura workspace aggiornata (necessario),
  sezione "Cosa NON è" riscritta (CLI/skill/UI ora presenti; restano fuori
  endpoint HTTP e FFI).
- AGENTS.md §3: stato con CLI/skill/UI e preview aggiornato.
- `scripts/preview.sh` + `scripts/render_report.py`: report con demo CLI
  (output JSON reale) e screenshot UI embedded; fix stale "Wasmtime 28".
- Fix delivery preview (seguente): il BMP non è più incorporato inline
  (pagina 312 KB → 3 KB); la pagina referenzia `ui-screenshot.bmp` come
  file separato con link di download. Motivo: viewer/tagli scroll su pagine
  HTML troppo grandi — l'immagine era presente ma non raggiungibile.

## 2026-10-09 — audit di completamento + OKF v0.2

- Prima riga dell'indirizzo in `ROADMAP.md`, `blueprint.md`,
  `github-advanced-security.md`: indicati con precisione tutti i punti
  verificati (Wasmtime 49.0, 27/27 test, CI+CodeQL verdi, benchmark reali,
  LICENSE-MIT/APACHE, README + badge, `examples/host-run`, bench solo-std).
- Nuovo documento OKF: `index.md` (directory listing) e questo `log.md`.
- AGENTS.md aggiornato: era stantio (wasmtime 28, 20/20, pendenti risolti come
  aperti). Ora allineato: baseline 27/27, wasmtime 49.0, `gnario` superato,
  crates.io (`wasmbox` occupato, `wasmbox-core` libreto), divieto anyhow.

## 2026-10-09 — v0.4 (aggiunte post-validazione approvate)

- `blueprint.md` → v0.4: `examples/host-run`, `benches/perf.rs` (solo std),
  7 test di robustezza (→ 27 totali), metadati crates.io con
  `cargo publish --dry-run` che esce 0.

## 2026-10-08 — v0.3 (migrazione wasmtime 28 → 49.0)

- `blueprint.md` → v0.3: `anyhow` rimosso, host function su `wasmtime::Result`,
  `IntoFunc` non accetta più `anyhow::Result`, `PoolingAllocationConfig`
  in byte non pagine.

## 2026-10-08 — prima stesura progetto

- `blueprint.md` v0.2 salvato prima del codice (richiesta esplicita utente).
- `ROADMAP.md`: cronologia sessione-by-sessione con prove reali.
