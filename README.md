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

**Resta fuori** (estensioni possibili, nessuna avviata — decisione aperta,
vedi `docs/ROADMAP.md` §2.9):

- **endpoint HTTP/gRPC** per l'uso da agenti AI esterni senza processo CLI;
- FFI/C-ABI per altri linguaggi.

La scelta è deliberata: il blueprint vieta logica di dominio nel crate `core`,
e un protocollo di interfaccia È logica di dominio — sta in un crate separato
sopra `wasmbox-core` (CLI e UI seguono già questa regola).

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

## Pubblicazione su crates.io

`wasmbox-core` è pronto (`cargo publish --dry-run` = 0, 12 file). Nota: il nome
`wasmbox` puro è già occupato su crates.io (2022) — il pacchetto si chiama
`wasmbox-core`.

## Licenza

MIT OR Apache-2.0 (vedi `Cargo.toml`).
