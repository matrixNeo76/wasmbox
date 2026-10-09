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

- **[docs/blueprint.md](docs/blueprint.md)** — specifica completa
  (architettura, ABI, limite per limite, ordine di implementazione)
- **[docs/github-advanced-security.md](docs/github-advanced-security.md)**
  — integrazione CodeQL / secret scanning / Dependabot

## Licenza

MIT OR Apache-2.0 (vedi `Cargo.toml`).
