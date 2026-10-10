---
type: Guide
title: "wasmbox — scenari d'integrazione via `ask`: pattern e ricette complete"
description: "Come costruire su wasmbox-core le soluzioni d'integrazione più diffuse usando la host function ask: backend FastAPI (proxy), gRPC/microservizi, Orchestrazione di agenti LLM, automazione con agenti AI via CLI, serverless/lambda, Python via FFI. Per ogni scenario: architettura, protocollo, codice, limiti, rischi, checklist."
resource: "docs/scenarios.md"
tags: ["scenarios", "integration", "ask-protocol", "fastapi", "grpc", "llm-agents", "serverless", "ffi", "patterns"]
updated: "2026-10-10"
---

# SCENARI D'INTEGRAZIONE VIA `ask` — pattern e ricette complete

> **Lettura per**: chi ha una soluzione in mente (backend REST, gRPC, agente
> LLM, automazione, serverless, FFI…) e vuole capire **come agganciarla a
> `wasmbox-core`** passando dall'unica capability del guest: la host
> function `env::ask`. Prende e specializza la "via libreria" descritta in
> [`integration.md`](integration.md) §2 con ricette concrete per i 6 scenari
> più diffusi; il meccanismo `ask` è spiegato lì (§"Protocollo lato host
> sopra `ask`") e qui si assume noto.
>
> **Regola d'oro** (valida per TUTTI gli scenari): il guest è un trasporto
> opaco del protocollo; **la logica di dominio sta SEMPRE nell'host handler**,
> mai nel guest e mai in `wasmbox-core` (vincolo del blueprint).

---

## 0) Come scegliere lo scenario (matrice di decisione)

| Il tuo caso | Scenario consigliato | Sezione |
|---|---|---|
| Backend Python/FastAPI (o Django/Flask), logica dominio in Python | **S1 — FastAPI call-back (ask-proxy)** | §1 |
| Microservizi Go / Node / Java / altro con contratto gRPC | **S2 — gRPC bridge** | §2 |
| Orchestrazione di agenti LLM con tool-calling (function calling) | **S3 — Agente LLM tool-bus** | §3 |
| Automazione/agenti scriptabili (n8n, workflow, cron, CI) | **S4 — CLI/subprocess con contratto JSON** | §4 |
| Serverless (AWS Lambda, Cloud Functions, Cloudflare Workers…)* | **S5 — Serverless / FFI in-process** | §5 |
| Data pipeline / Jupyter / batch analytics in Python | **S6 — Python in-process FFI** | §6 |

\* S5 dipende dal provider: dove non ci sono processi (`Cloudflare Workers`)
wasmbox non gira — vedere i limiti in §5.

---

## 1) S1 — FastAPI backend con logica Python: pattern ask-proxy

**L'idea in una frase**: la sandbox gira in un processo Rust separato
(`wasmbox-http` o un crate tuo); l'handler Rust NON contiene la logica
dominio: la **inoltra** alla tua FastAPI su loopback e incolla la risposta.

Perché: wasmbox gestisce (core, guest, engine) e FastAPI gestisce (regole,
DB, LLM). Il guest non capisce nulla di entrambi: chiede via `ask`, l'host
Rust riversa la richiesta al backend Python, Python decide, la risposta
torna all'indietro sui stessi binari.

```
browser/servizi      FastAPI (tuo dominio, Python)
     │                        ▲
     │ POST /api/execute      │ loopback :8001  (handler tool_call)
     ▼                        │
┌── wasmbox-http :8130 ───────┴──┐
│ handler ask → HTTP → FastAPI   │
│ (proxy, niente logica qui)     │
└──────┬─────────────────────────┘
       │ ask(req_ptr, req_len)
       ▼
┌──── guest wasm ────────────────┐
│ trasporta req e resp via ask   │
│ esegue SOLO calcolo di dominio │
└────────────────────────────────┘
```

### 1.1 Architettura

- **`wasmbox-http`** (già nel repo, scenario D): server sandbox su loopback
  `127.0.0.1:8130`, rotta `POST /run` (guest+input base64 → output base64) +
  `GET /healthz`.
- ESTENSIONE richiesta (cosa c'è da scrivere, ~150 righe Rust): un
  **`ProxyHandler`** nel crate `wasmbox-http` (feature `proxy` opt-in) che,
  invece dell'eco, fa `POST http://127.0.0.1:8001/tool_call` con corpo =
  i byte della richiesta `ask`, e risponde nel guest con i byte letti dalla
  risposta HTTP. La FastAPI è l'**unica fonte di verità** (DB, auth, LLM…).
- Il protocollo tra guest e backend è SEMPRE opaco al core: potete
  usarlo come JSON, MessagePack o binario, senza che wasmbox se ne
  accorga.

### 1.2 Ricetta passo-passo

1. **Rust** — new crate `wasmbox-proxy` (o feature in `wasmbox-http`):

   ```rust
   use wasmbox_core::{HostError, HostHandler};

   pub struct ProxyHandler { endpoint: String, client: ureq::Agent }

   impl HostHandler for ProxyHandler {
       fn ask(&mut self, request: &[u8]) -> Result<Vec<u8>, HostError> {
           let resp = self.client.post(self.endpoint.as_str())
               .send_bytes(request)               // byte opachi in pass-thru
               .map_err(|e| HostError::Custom(format!("proxy: {e}")))?;
           // facoltativo: check http status → HostError
           resp.into_body().read_to_vec()
               .map_err(|e| HostError::Custom(format!("proxy resp: {e}")))
       }
   }
   ```

2. **Python/FastAPI** lato dominio (esempio di un tool `soma` + rule engine):

   ```python
   from fastapi import FastAPI, Request
   from pydantic import BaseModel
   app = FastAPI()

   @app.post("/tool_call")
   async def tool_call(req: Request):
       raw = await req.body()               # stessi byte che il guest ha scritto
       # 1) decodifica il TUO protocollo (es. prima riga = comando)
       cmd, _, arg = raw.partition(b':')
       # 2) fai ciò che vuoi con il DOMINIO: DB, auth, LLM, regole…
       if cmd == b'soma':
           a, b = map(int, arg.split(b'x')); result = a + b
       elif cmd == b'user_score':
           result = db.score(arg.decode())  # accesso DB vero
       else:
           return b'tool_result:err=unknown-tool'
       return b'tool_result:' + str(result).encode()
   ```

3. **FastAPI → sandbox** (punto di entrata per i client):

   ```python
   @app.post("/api/execute")
   async def execute(payload: ExecuteIn):
       async with httpx.AsyncClient(base_url="http://127.0.0.1:8130") as c:
           r = await c.post("/run", json={
               "guest": b64(guest_wasm_bytes),      # cache-side dal repo
               "input": b64(payload.user_input),
               "limits": {"max_ask_calls": 64, "max_fuel": 50_000_000}})
       r.raise_for_status()
       return MapOutput(ok=r.json()["ok"], output=b64dec(r.json()["output"]))
   ```

4. **Guest**: serve un guest che parla il TUO protocollo (usa
   `examples/scenario-tool-handler/guest` come template): compone
   `tool:<cmd>:<arg>`, la manda ad `ask`, decodifica `tool_result:…`.

### 1.3 Limiti/rischi e mitigazioni

- **Latency**: ogni `ask` = un round-trip guest→handler→FastAPI. Con
  `max_ask_calls` sotto controllo e connessione loopback (sub-millisecondo)
  la latenza tipica resta < 2 ms; lo stesso criterio vale per i DB:
  connettendosi con connection-pool dentro FastAPI (mai dal proxy Rust).
- **Sicurezza**: `wasmbox-http` niente auth/TLS in v0.7 → bind loopback e
  reverse-proxy davanti per l'esposizione agli utenti (in S1 la sandbox
  NON è pubblica: la sua rotta /run è chiamata SOLO dalla tua FastAPI).
- **Back-pressure**: la rotta `/run` è blocking (una richiesta = max
  epoch_timeout di run); per carico alto: processi wasmbox-http multipli
  dietro un round-robin (systemd socket-activation oppure stack di
  container) e un pool di connessioni da FastAPI.
- **Cosa NON fare**: logica di dominio nel proxy Rust (solo pipe); auth del
  backend gestita da token FastAPI, mai nel guest; input utente passato
  direttamente come guest input senza validazione.

### 1.4 Criteri di accettazione

- `scenario-d.sh` PASS (sandbox alla sua baseline)
- con un guest `tool:` reale: `tool_result:` torna al client via `/api/execute`
- `max_ask_calls` rispetto: guest che ne abusa → `ask_limit_exceeded`, mai
  crash né bypass del backend.

---

## 2) S2 — Microservizi/gRPC: bridge con proto esplicito

**Caso**: i tuoi servizi parlano già gRPC (Go/Java/Node), e vuoi che un
guest wasm invochi qualche RPC. Due architetture possibili:

### 2.1 variante "handler RPC" (consigliata)

`wasmbox-http`-side handler che invoca direttamente il gRPC target (crate
`tonic` in Rust) senza passare dalla tua app:

```rust
impl HostHandler for GrpcHandler {
    fn ask(&mut self, request: &[u8]) -> Result<Vec<u8>, HostError> {
        // deserializza: proto UserScoreRequest dai byte del guest
        let req = proto::UserScoreRequest::decode(request)
            .map_err(|e| HostError::Custom(format!("proto: {e}")))?;
        let resp = self.rt.block_on(self.client.user_score(tonic::Request::new(req)))
            .map_err(|e| HostError::Custom(format!("grpc: {e}")))?;
        let mut out = Vec::new();
        resp.get_ref().encode(&mut out)
            .map_err(|e| HostError::Custom(format!("proto enc: {e}")))?;
        Ok(out)
    }
}
```

**Invariante da rispettare**: wasmbox-core esegue su un thread e NON abbandona
il chiamante — niente runtime tokio multi-thread dentro `ask`. La ricetta
validata:
crea un runtime `tokio::runtime::Builder::new_current_thread` per l'handler
(costa poco) e usa `.blocking_on(client.user_score(...))`. Nessuna eccezione:
niente future async sparse, in questa versione.

### 2.2 variante "HTTP proxy" (più semplice, raccomandata per v0.7)

Il gRPC si può riportare sopra HTTP/JSON (grpc-gateway o shim FastAPI come S1);
poi si riusa S1 così-com'è. Meno trappole.

---

## 3) S3 — Agente LLM tool-bus: function calling dove il guest è il "modello"

**Caso**: vuoi che un agente (Claude/GPT/LLM locale) **esegua codice
guest** in sandbox e che il guest possa chiamare i tool dell'agente
(ricerca web, DB, documenti) via `ask`. Al contrario dello scenario F
(che mette la chiamata LLM nell'handler), QUI il flusso è invertito.

```
Agente → decide tool+argomenti → genera guest wasm (oppure riusa un guest
        "chat-run" dalla tool-box) → POST /run → il guest parla con i tool
        via ask → risultato → l'agente valuta → itera.
```

### 3.1 Ricetta

1. **Tool-bus in FastAPI** (S1 `ProxyHandler` o via libreria):
   gestisci `tool:<name>:<json_args>` con un dispatch JSON-native
   per la risposta (JSON pur essendo byte opachi per il core, ok).

2. **Sandbox config per i tool agent**:
   ```python
   "limits": {
       "max_fuel": 200_000_000,        # task agente LLM pesante
       "epoch_timeout_ms": 5000,       # wall-cap a 5 s
       "max_ask_calls": 48,            # tool call per singola run
       "max_ask_payload_bytes": 65536
   }
   ```

3. **Idempotenza e side-effect**: i tool con side-effect (write DB)
   vivono SOLO nel tool-bus, MAI nel guest, con log di side-effect e
   idempotency-key per ogni tool chiamato.
4. **Guest cache**: i guest "wasm generato per task" vanno dedup con SHA-256
   (cache_dir = `SandboxConfig.cache_dir` — 1.355 ms → 0.136 ms misurati).

### 3.2 Perché non il contrario

Lo scenario F (`llm:<prompt>` nell'handler) è per **autosufficienza** del
guest in isolamento (senza backend); S3 è per agenti orchestrati che
affidano al backend tool reali. Sono complementari: puoi avere ENTRAMBI
con la stessa `ask` (dispatch per prefisso: `llm:` vs `tool:` vs `auth:`).

---

## 4) S4 — CLI/subprocess: automazione con un contratto JSON (GIÀ ONLINE)

**Quando**: workflow (n8n, GitHub Action, cron), agenti AI esterni e script
bash dove un processo è sufficiente. Nessun nuovo codice: questa via esiste
e è la preferita per agenti esterni (skill `skills/wasmbox/SKILL.md`).

```bash
out=$(wasmbox-cli run guest.wasm "$INPUT" --json)
code=$?
case $code in
  0) echo "output: $out" ;;
  3|4|5) echo "resource limit — don't retry identical" ;; # policy
  6) echo "ask abuse" ;;
  7) echo "host handler failed" ;;
  8|9) echo "guest malformed (ABI/wasm)" ;;
  *) echo "generic: $out" ;;
esac
```

- Per un wrapper Python:
  ```python
  import subprocess, json
  r = subprocess.run(["wasmbox-cli", "run", guest, input, "--json"],
                     capture_output=True, text=True, timeout=30)
  result = json.loads(r.stdout)
  if r.returncode == 0: ...       # result["output"]
  elif 3 <= r.returncode <= 6: ...  # limite/rifiuto — NON ritentare
  ```

- **Regola operativa** (dal contratto skill): mai ritentare un identico run
  dopo 3/4/5/6; correggere il guest o l'handler, non il runtime.

---

## 5) S5 — Serverless: cosa cambia e quando NON farlo

| piattaforma | gira wasmbox? | nota |
|---|---|---|
| AWS Lambda (container image / bootstrap) | **sì** (binario Rust su linux arm64/x86_64) | pattern: `wasmbox-http` dentro la Lambda (bootstrap) o FFI (S6 in-process) |
| Cloud Functions (Google/GCP) | **sì**, con `--runtime` custom (binario) | esporre /run via adapter HTTP |
| Cloudflare Workers | **no** (runtime wasm CUSTOM, niente processi) | se serve sandbox su Workers, valutare wasmtime/js core o un'estensione futura |
| AWS Fargate / Cloud Run | **sì** (container liberi) | pattern completo S1/spawn container |

**Caveat chiave (cold-start)**: `SandboxEngine::new` con cache-freddo ≈
1.355 ms — la compile per singola-invoke NON è un problema. La cache `.cwasm`
non persiste tra istanze isolate (filesystem effimero): per il cold path accettarlo.
**Non montare** `/tmp` condiviso come cache (non portabile tra arch).

---

## 6) S6 — Python in-process FFI (Jupyter, pandas, batch)

**Quando**: pipeline in Python con latenza minimissima e NESSUNA
per la rete; doc → `wasmbox-ffi` (`cdylib`) via `ctypes`.

```python
# passo-passo completo: scripts/scenarios/scenario-e.sh
lib = ctypes.CDLL("target/release/libwasmbox_ffi.so")
# config limits struct opzionale
eng = lib.wasmbox_engine_new(wasm_bytes, len(wasm_bytes), None)
assert eng  # NULL su wasm invalido
out_ptr, out_len = (ctypes.c_void_p(), ctypes.c_size_t())
st = lib.wasmbox_engine_run(eng, in_bytes, len(in_bytes),
                            ctypes.byref(out_ptr), ctypes.byref(out_len))
if st == 0:
    data = ctypes.string_at(out_ptr, out_len)
    lib.wasmbox_buffer_free(out_ptr, out_len)  # SEMPRE: memoria crate
elif st == 3:
    # fallita: last_error()/last_error_code() → exit-style 3..10
    msg = ctypes.c_char_p(lib.wasmbox_last_error(eng)).value
```

**Vincoli essenziali**:
- `engine` NON thread-safe → una per worker (o un semplice `_lock`).
- Handler `ask` della FFI v0.7 = SOLO eco → in-process niente dominio dietro
  `ask`: se il guest ha bisogno di tool veri, passa a S1 (proxy); e usa
  `concurrent.futures.ProcessPoolExecutor` per non bloccare il main-thread.
- `wasmbox_last_error` punta alla memoria del crate: **copiare** la stringa
  (`.value` di ctypes copia) prima di qualunque altra chiamata.

---

## 7) Riepilogo: la stessa `ask` potenzia TUTTI gli scenari

Ogni soluzione cambia SOLO (a) la serializzazione del protocollo opaco e
(b) la logica nell'`HostHandler`. La sandbox (fuel, memory, timeout,
budget chiamate — numeri nel benchmark perf) resta IDENTICA in ogni
scenario: è questo il vantaggio di wasmbox-core: un'unica porta, sempre
gli stessi profili di errore.

| Scenario | Handler | Protocollo | Logica dominio |
|---|---|---|---|
| S1 FastAPI | Rust proxy → FastAPI | binario/JSON | dominio in Python |
| S2 gRPC | Rust → tonic RPC | proto serialized | dominio in Go/Java |
| S3 LLM agents | Rust proxy (come S1) | `tool:<cmd>:<json>` | tool degli agenti |
| S4 CLI | nessun handler custom | testo | script/agenti semplici |
| S5 Serverless | S1 oppure FFI | — | deploy compresso |
| S6 FFI | eco in-crate | bytes nativi | zero dipendenze |

## 8) Checklist comune PRIMA di mettere in produzione

- [ ] Il guest NON contiene logica di dominio (solo trasporto della
      richiesta) — confronto con `examples/scenario-tool-handler/guest`.
- [ ] Un test "guest ostile" (fuel-bomb, ask-flood) ancora attivo:
      `sh scripts/scenarios/scenario-c.sh` — exit tipizzato, MAI crash.
- [ ] `limits` espliciti per ambiente (produzionale thường ≠ sandbox
      playground): `max_ask_calls` in-of (S1: 32–128; S3: 48+).
- [ ] Reverse-proxy + auth davanti a wasmbox-http SE non solo loopback
      (S1); mai `0.0.0.0` senza auth.
- [ ] Ogni errore handler è `SandboxError::Host` verso FastAPI: mai crash
      del processo host, mai messaggi con dati sensibili.
- [ ] Log di sessione `ask` (chiamate, byte, durata) nel tuo host
      handler per billing/diagnostica — il core non logga nulla
      (zero dipendenze, silence-by-default: logga tu).

## Riferimenti

- Meccanismo `ask` + ABI + mappa errori: [`integration.md`](integration.md)
- Endpoint HTTP già nel repo: [`/crates/wasmbox-http`](../crates/wasmbox-http/)
- FFI già nel repo: [`/crates/wasmbox-ffi`](../crates/wasmbox-ffi/)
- CLI per agenti: [`skills/wasmbox/SKILL.md`](../skills/wasmbox/SKILL.md)
- Esempi implementati: scenari A–F in [`../examples/`](../examples/) e
  [`../scripts/scenarios/`](../scripts/scenarios/)
- Benchmark: [`perf.rs`](../crates/wasmbox-core/benches/perf.rs)
