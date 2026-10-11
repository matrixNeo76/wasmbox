# wasmbox

[![CodeRabbit Pull Request Reviews](https://img.shields.io/coderabbit/prs/github/matrixNeo76/wasmbox?utm_source=oss&utm_medium=github&utm_campaign=matrixNeo76%2Fwasmbox&labelColor=171717&color=FF570A&link=https%3A%2F%2Fcoderabbit.ai&label=CodeRabbit+Reviews)](https://coderabbit.ai)
[![CI](https://github.com/matrixNeo76/wasmbox/actions/workflows/ci.yml/badge.svg)](https://github.com/matrixNeo76/wasmbox/actions/workflows/ci.yml)
[![CodeQL](https://github.com/matrixNeo76/wasmbox/actions/workflows/codeql.yml/badge.svg)](https://github.com/matrixNeo76/wasmbox/actions/workflows/codeql.yml)

**Esecutore WebAssembly isolato ad alte prestazioni** — un workspace Rust che
permette a un'applicazione host di eseguire codice non fidato (plugin, script
generati da LLM, agenti decisionali) in un ambiente dove il guest:

- **non ha accesso** a filesystem, rete o altre risorse di sistema;
- **non può consumare** risorse illimitate (CPU, memoria, tempo);
- **può comunicare** con l host solo tramite una singola funzione generica,
  opaca: `env::ask(req_ptr, req_len) -> i64`.

Il runtime è basato su **[Wasmtime 49](https://wasmtime.dev/)** con fuel
metering, epoch interruption (timeout wall-clock), `StoreLimits` espliciti,
pooling allocator e cache `.cwasm` precompilata.

## Principio architetturale

> `wasmbox-core` non sa nulla del mondo esterno. Sa solo eseguire Wasm con
> limiti di risorsa e inoltrare richieste opache a un `HostHandler` fornito
> dall host.

La logica di dominio vive interamente nell'host: il runtime non interpreta
né la richiesta né la risposta di `ask` — sono bytes opachi.

## Perché è la scelta giusta per tool generati da LLM

Gli LLM generano codice che può essere sbagliato o ostile **per costruzione**:
wasmbox lo rende eseguibile in sicurezza esattamente quando un agente dovrebbe
eseguirlo, cioè senza fiducia dichiarativa ma con veri limiti:

- **Niente di altro nel runtime**: il guest non vede filesystem, rete,
  processi, né risorse di sistema — non c'è WASI abilitato da default: il
  guest non può *aprire* nulla, non solo *non è autorizzato*; l'unica porta
  verso fuori è `env::ask` (bytes opachi, sottoposti a budget);
- **Il tool-bus resta nell'host**: l'agente (o il backend che lo orchestra)
  implementa `HostHandler::ask` e mantiene il controllo; il guest non può
  eseguire chiamate dirette verso nulla;
- **Limiti dati a priori**: applicati a *ogni* run: fuel (CPU), timeout
  wall-clock (epoch), memoria e budget di chiamate `ask`; un guest generato
  da LLM che si blocchi in un loop o provi un burst di chiamate esce con un
  errore **tipizzato** — mai crash né escape lato processo;
- **Determinismo dell'errore**: la stessa causa produce sempre lo stesso
  exit code (CLI 0/3/4/5/6/7/8/9/10) — un agente può valutare l'esito senza
  interpretare una traccia (`--json`, politiche in `skills/wasmbox/SKILL.md`);
- **Sei vie d'uso già provate**: come CLI (`--json`), via libreria, via
  HTTP (`wasmbox-http`), via FFI (Python/Node/Go), e in due modi con
  agenti LLM: scenario F (`llm:<prompt>` nell'handler) e
  [scenario S3](docs/scenarios.md) (agente tool-bus: il tuo orchestratore
  decide il run, il guest chiama i tuoi tool esclusivamente via `ask`).

## Struttura

```
├── crates/wasmbox-core/     # il motore: config, error, memory, engine
│   ├── tests/e2e_test.rs    # suite test (27: 5 unit + 22 e2e)
│   └── benches/perf.rs      # benchmark solo-std (cargo bench)
├── crates/wasmbox-cli/      # CLI per umani e agenti AI (--json, exit code)
├── crates/wasmbox-ui/       # UI minimale Slint (screenshot BMP headless)
├── skills/wasmbox/SKILL.md  # skill per agenti AI: come usare la CLI
├── examples/guest-echo/     # guest Wasm di esempio (wasm32-unknown-unknown)
├── examples/host-run/       # esempio lato host: carica un guest, passa un handler
├── examples/scenario-cli-pipeline/  # scenario A: pipeline fiducia via CLI
├── examples/scenario-tool-handler/  # scenario B: handler tool via libreria
├── examples/scenario-hostile-guest/ # scenario C: bombe ostili (i limiti in azione)
├── docs/blueprint.md        # specifica completa di progetto
├── docs/github-advanced-security.md
└── scripts/                 # install / build / preview / verifica
```

## Quickstart

```sh
# test della suite (richiede Rust stable)
cargo test -p wasmbox-core

# build del guest di esempio per wasm32
cargo build -p guest-echo --target wasm32-unknown-unknown --release

# validazione completa
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check

# esempio host (dopo aver buildato il guest)
cargo run -p host-run -- target/wasm32-unknown-unknown/release/guest_echo.wasm "ciao"

# benchmark (compile freddo/cache, overhead di ask)
cargo bench -p wasmbox-core
```

### Benchmark pubblici — i limiti MISURATI, non dichiarati

**Differenza competitiva**: i competitor (Eryx, sandboxd, Sandlock)
pubblicano cifre "dichiarate dal progetto", non riproducibili da terzi.
I nostri numeri sono **riproducibili da chiunque** con un solo comando:

```sh
cargo bench -p wasmbox-core -- --nocapture
```

Il bench (`benches/perf.rs`, `harness = false`, solo std + `wat` in
dev-dep) misura gli stessi numeri citati in tutto il repo:

| Grandezza | Valore di riferimento (2026-10-11, x86_64 Linux) |
|---|---|
| Compile a freddo (`SandboxEngine::new`) | ~1.7–2.4 ms |
| Compile con cache hit (`.cwasm`) | ~0.13–0.26 ms |
| Run con round-trip `ask` | ~0.7 µs overhead sopra run senza `ask` |
| Round-trip `ask` payload 64B | ~10 µs/run (n=500) |
| Round-trip `ask` payload 4KB | ~12 µs/run (n=500) |
| Round-trip `ask` payload 64KB | ~48 µs/run (n=500) |
| Round-trip `ask` payload 1MB | ~580 µs/run (n=500) |
| Costo marginale oltre 64B | ~0.55 ns/byte (copia RAM→RAM) |
| **Rifiuto tipizzato** (payload 1 MiB+1 > tetto 1 MiB) | **~535 ns, errore `PayloadTooLarge` ENFORCED, mai crash** |

L'ultima riga è la prova che i limiti **funzionano e sono economici da
far rispettare**: la sandbox rifiuta un payload oltre il tetto con un
errore tipizzato in mezzo microsecondo, PRIMA di toccare il guest — non
un crash, non un escape, sempre lo stesso errore (`--json` in CLI:
exit 6 deterministico).

Caveat di onestà: i numeri sono **macchina-dipendenti**; vanno citati con
versione/target e sistema. La CI **li produce a ogni push su `main`**
(job `benchmark` in `.github/workflows/ci.yml`): scarica l'artifact
`bench-report` dall'ultima run d'actions per i numeri aggiornati — sempre
riproducibili e datati, rispetto a un benchmark scritto one-off.

### CLI (per umani e agenti AI)

```sh
cargo build -p wasmbox-cli

# output umano (stdout = output del guest)
target/debug/wasmbox-cli run target/wasm32-unknown-unknown/release/guest_echo.wasm "ciao"
# → echo_result:INSPECT:CIAO

# pipe/stdin per agenti e script
echo -n "input da pipe" | target/debug/wasmbox-cli run <guest.wasm>

# output JSON machine-readable (UNA riga su stdout) + exit code deterministici
target/debug/wasmbox-cli run <guest.wasm> "ciao" --json
# → {"ok":true,"output":"echo_result:INSPECT:CIAO"}
```

Exit code: 0 ok · 2 uso/IO · 3 fuel · 4 timeout · 5 memoria guest · 6 ask
limit/payload · 7 errore handler · 8 export mancante · 9 wasm invalido ·
10 esecuzione. Contratto completo per agenti: [`skills/wasmbox/SKILL.md`](skills/wasmbox/SKILL.md).

> **Nota agenti**: per i coding agent la via primaria è **CLI + skill**
> (sopra); il server MCP (sotto) serve gli host agentici senza shell
> (Claude Desktop, LangGraph, agenti custom). Vedi
> [`docs/agent-loop-plan.md`](docs/agent-loop-plan.md).

### MCP server (per host agentici senza shell)

```sh
cargo build -p wasmbox-mcp
# Server MCP stdio persistente (spec MCP 2026-07-28), un solo tool:
# `wasmbox_run` (guest base64, envelope JSON capato 4 KiB + head/tail).
# Per Claude Desktop, Cursor, LangGraph, agent custom: da config MCP,
# tipo { "command": "./target/debug/wasmbox-mcp" }
target/debug/wasmbox-mcp
# → initialize / tools/list / tools/call (vedi docs/agent-loop-plan.md §2.4)
```

Envelope: `{"ok":true,"output":"…","ask_calls":0,"duration_ms":2,"output_truncated":false}`
(evento `output_truncated:true` con head+tail oltre il cap) o
`{"ok":false,"error":"fuel_exhausted","message":"…",…}` — stessi errori
snake_case della CLI. Limiti SEMPRE attivi: l'agente può solo restr
ingere (max_fuel, timeout_ms). Contratto completo:
[`docs/agent-loop-plan.md`](docs/agent-loop-plan.md).

### UI minimale (Slint, headless)

```sh
cargo build -p wasmbox-ui
# screenshot BMP 320×240: verde = run guest ok, rossa = fallita
target/debug/wasmbox-ui --screenshot screenshot.bmp \
  target/wasm32-unknown-unknown/release/guest_echo.wasm "ciao"
```

## Contratto ABI (guest)

| Export | Firma | Scopo |
|---|---|---|
| `memory` | `(memory 1)` | memoria lineare condivisa |
| `guest_alloc` | `(i32) -> i32` | alloca `len` byte (0 ⇒ dangling non-null) |
| `guest_free` | `(i32 i32)` | libera (no-op se `len == 0`) |
| `guest_run` | `(i32 i32) -> i64` | entry point: input ⇒ output packed |

| Import | Firma | Scopo |
|---|---|---|
| `env::ask` | `(i32 i32) -> i64` | richiesta all'host, risposta packed `(ptr << 32) \| len` |

Errori tipizzati: `FuelExhausted`, `Timeout`, `GuestOutOfMemory`,
`AskLimitExceeded`, `PayloadTooLarge`, `MissingExport`, `Host(...)` — vedi
`crates/wasmbox-core/src/error.rs`.

## Documentazione

La documentazione in `docs/` segue il formato **[OKF v0.2](https://github.com/GoogleCloudPlatform/open-knowledge-format)** (Open Knowledge Format: Markdown + frontmatter YAML, `index.md` + `log.md`):

- **[docs/index.md](docs/index.md)** — indice del bundle (OKF v0.2)
- **[docs/blueprint.md](docs/blueprint.md)** — specifica vincolante
  (architettura, ABI, limite per limite, divieti, ordine di implementazione)
- **[docs/ROADMAP.md](docs/ROADMAP.md)** — dove siamo: stato verificato,
  prossimi passi, cronologia con prove, trappole note
- **[docs/github-advanced-security.md](docs/github-advanced-security.md)**
  — integrazione CodeQL / secret scanning / Dependabot
- **[docs/comparative-analysis.md](docs/comparative-analysis.md)** —
  analisi comparativa fact-checked con le altre sandbox (Firecracker,
  E2B, gVisor, Sandlock, Extism, RLBox, Eryx, sandboxd), threat model e
  caso studio CVE-2026-35195

## Cosa wasmbox NON è (audit 2026-10-09, aggiornato)

Il **crate `wasmbox-core` è e resta una libreria Rust** — niente logica di
dominio, niente protocolli, niente I/O: si usa via `SandboxEngine::new` +
`engine.run(input, &mut handler)` (vedi `examples/host-run`).

Le **interfacce di fruizione** vivono in crate applicativi separati, tutti
già presenti nel workspace (introdotte il 2026-10-09):

- **`crates/wasmbox-cli`** — CLI per umani e agenti AI (run + `--json`,
  exit deterministici, handler eco);
- **`crates/wasmbox-ui`** — UI minimale Slint con screenshot BMP headless;
- **`skills/wasmbox/SKILL.md`** — contratto per agenti AI.

**Estensioni v0.7 (verificate 2026-10-10, spec in `docs/extension-plan.md`)**:

- **`crates/wasmbox-http`** — endpoint HTTP per orchestratori remoti:
  `wasmbox-http [--bind 127.0.0.1:8130]`, rotta unica `POST /run`
  (guest+input base64, limiti di risorsa per-request) + `GET /healthz`;
  server std, zero dipendenze oltre al core, JSON/base64 scritti a mano.
  Prova: `sh scripts/scenarios/scenario-d.sh` (4/4 PASS).
- **`crates/wasmbox-ffi`** — C-ABI stabile per Python/Node/Go/Android/iOS
  (`cdylib+staticlib`): engine new/free/run + `wasmbox_buffer_free` +
  `last_error(_code)` con exit code stile CLI. Prova con python3 stdlib:
  `sh scripts/scenarios/scenario-e.sh` (5/5 PASS).
- **Tool LLM reale** (scenario F, feature opt-in `llm` su
  `examples/scenario-tool-handler`, `ureq`): `llm:<prompt>` via OpenRouter —
  la rete sta SOLO nell'host handler; senza `OPENROUTER_API_KEY` → exit 7
  tipizzato. Prova offline: `sh scripts/scenarios/scenario-f.sh`.
- **WASI opt-in** (`crates/wasmbox-wasi`, 2026-10-11): layer **separato**
  dal core con capability **deny-by-default** — `WasiCapabilities::default()`
  = zero preopen, zero clock/random; un guest che importa syscall non
  concesse NON istanzia. Con preopens readonly e stdout captured il guest
  può leggere file concessi e stampare (catturato in `run`). Fuel/epoch/
  memoria/`ask` invariati. Spec: [`docs/wasi-plan.md`](docs/wasi-plan.md),
  test `cargo test -p wasmbox-wasi` (7/7 verdi).

La regola resta: il blueprint vieta logica di dominio e I/O nel crate `core` —
tutto sta in crate separati sopra `wasmbox-core` (CLI, UI, http e FFI
seguono già questa regola).

## Release GitHub (binari scaricabili)

Ogni tag `vX.Y.Z` → release automatica con binari precompilati per
**quattro piattaforme** (v0.6): linux x86_64, macOS Apple Silicon e Intel,
Windows x64 MSVC. Ogni pacchetto contiene `wasmbox-cli`, `wasmbox-ui`,
`guest_echo.wasm` + README e licenze, con checksum SHA-256. Il workflow
(`.github/workflows/release.yml`) esegue smoke test CLI+UI **su ogni
sistema** prima di pubblicare.

```sh
git tag v0.5.1
git push origin v0.5.1   # build + release automatica (4 piattaforme)
```

I binari si usano così (nessun Rust richiesto):

```sh
tar xzf wasmbox-v0.5.1-x86_64-unknown-linux-gnu.tar.gz
cd wasmbox-v0.5.1-x86_64-unknown-linux-gnu
./wasmbox-cli run guest_echo.wasm "ciao" --json
./wasmbox-ui --screenshot ui.bmp guest_echo.wasm "ciao"
```

Su macOS: come linux ma le release portano `aarch64-apple-darwin` (Apple
Silicon) e `x86_64-apple-darwin` (Intel). Su Windows: pacchetto `.zip` con
`wasmbox-cli.exe`/`wasmbox-ui.exe` (`Expand-Archive`), target `x86_64-pc-windows-msvc`.

## Scenari di integrazione (esempi provabili)

Tre scenari eseguibili dimostrano [la guida d'integrazione](docs/integration.md):

```sh
sh scripts/scenarios/scenario-a.sh        # A: un agente decide TRUST/REJECT da exit+JSON
sh scripts/scenarios/build-scenarios.sh   # compila guest+B e bombe C
cargo run -p scenario-tool-handler -- \
  target/scenario-hostile/scenario_tool_guest.wasm "soma:7x35"   # B: tool_result:soma=42
sh scripts/scenarios/scenario-c.sh        # C: fuel/OOM/ask-limit → errore tipizzato, mai crash
sh scripts/scenarios/scenario-d.sh        # D: POST /run via HTTP — echo, 400, invalid_wasm, fuel deterministico
sh scripts/scenarios/scenario-e.sh        # E: FFI via python3 ctypes — engine new/run/free senza dipendenze
sh scripts/scenarios/scenario-f.sh        # F: tool llm senza chiave → exit 7 tipizzato (SKIP online senza OPENROUTER_API_KEY)
```

## Pubblicazione su crates.io

`wasmbox-core` è pronto (`cargo publish --dry-run` = 0, 12 file). Nota: il nome
`wasmbox` puro è già occupato su crates.io (2022) — il pacchetto si chiama
`wasmbox-core`.

## Licenza

MIT OR Apache-2.0 (vedi `Cargo.toml`).
