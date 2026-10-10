---
type: Specification
title: "wasmbox — EXTENSION PLAN v0.7: mitigazione M1 + scenari D/E/F"
description: "Spec/piano da approvare prima del codice: (M1) mitigazione del non-determinismo fuel/timeout nello scenario C, (D) endpoint HTTP per orchestrazione remota in crate separato, (E) FFI/C-ABI, (F) tool LLM reale via OpenRouter nello scenario B."
resource: "docs/extension-plan.md"
tags: ["spec", "extensibility", "http", "ffi", "openrouter", "scenarios"]
updated: "2026-10-10"
---

# EXTENSION PLAN — mitigazione M1 + scenari D/E/F (v0.7, da approvare)

> Spec **proposta** per i tre punti lasciati aperti (ROADMAP §2.9 opzioni
> (b)/(c) + LLM reale) e per il non-determinismo dichiarato nello scenario C.
> **Nessun codice finché l'utente non conferma.** Sequenza: spec → conferma →
> implementazione → baseline verde → push → CI/CodeQL.

---

## 1) M1 — fuel-bomb non deterministica (exit 3 vs 4): causa e mitigazione

**Fatto misurato**: un loop infinito esce **3** (`fuel_exhausted`) oppure
**4** (`timeout`), a seconda del carico della macchina — il fuel budget
(1e9 istruzioni) e l'epoch wall-clock (1 s) corrono in parallelo e vince il
primo che scatta. Il runner `scenario-c.sh` lo accetta (3 O 4) ed è
documentato nel README; è onesto ma è un edge case eliminabile alla radice.

**Causa reale**: la `SandboxConfig` default ha fuel E timeout attivi
*insieme*. La mitigazione non tocca `map_guest_error` né appiattisce errori
(divieto blueprint n.7 resta valido).

**Mitigazione proposta (zero cambi all'ABI, zero flag CLI)**:
- un mini-crate `scenario-limit-runner` (stesso pattern dello scenario B:
  usa `SandboxEngine::run` direttamente) con una precisa configurazione
  `SandboxConfig { max_fuel: Some(5_000_000), epoch_timeout: None }`:
  con il timeout disattivato l'unico limite possibile è il fuel ⇒
  **exit 3 deterministico** per la fuel-bomb;
- `scenario-c.sh` esegue la bomba su due run: (a) engine default via
  `wasmbox-cli` (accetta 3 o 4, come oggi) e (b) engine fuel-only via
  `scenario-limit-runner` (esige esattamente 3);
- README aggiornato: la deterministica è la run b), la a) dimostra i limiti
  di default.

**Alternativa rifiutata**: esporre `--fuel/--timeout` in `wasmbox-cli` —
allargherebbe la superficie CLI deliberatamente minima (blueprint v0.5);
se mai servisse, si aggiorna prima il blueprint.

---

## 2) Scenario D — endpoint HTTP per orchestrazione remota (crate separato)

Perché: ROADMAP §2.9 opzione (b) era il gap dichiarato — orchestratori
remoti e host non-Rust oggi hanno solo la CLI. La via HTTP serve
"invia guest.wasm + payload da remoto, ricevi output/exit/error", con i
limiti di risorsa comunque fatti rispettare dalla sandbox.

### 2.1 Crate

```
crates/wasmbox-http/      # membro workspace, binario `wasmbox-http`
```

Dipendenze: **solo `wasmbox-core`** + std (server fatto a mano: `TcpListener`
+ thread per connessione; niente tokio/axum/hyper — rispetta la disciplina
dipendenze del workspace; il parsing JSON del protocollo v0.7 è scritto a
mano, senza serde). Nessun TLS in v0.7: HTTPS dietro reverse-proxy è
responsabilità dell'orchestratore (documentato nel README del crate).

### 2.2 API (una sola rotta)

```
POST /run
Content-Type: application/json
{
  "guest": "<base64 wasm>",      // obbligatorio, <= 32 MiB
  "input": "<base64|null>",      // default null
  "limits": {                    // opzionale, campi di SandboxConfig
    "max_fuel": <int|null>, "epoch_timeout_ms": <int|null>,
    "max_memory_bytes": <int>, "max_ask_calls": <int>,
    "max_ask_payload_bytes": <int>
  }
}
→ 200 {"ok":true, "output":"<base64>", "ask_calls":<int>}
→ 200 {"ok":false, "error":"<snake_case della CLI>", "message":"…"}
→ 400 {"ok":false,"error":"bad_request"}        // json malformato / guest mancante
→ 413 {"ok":false,"error":"guest_too_large"}
→ 404 {"ok":false,"error":"not_found"} ; 405 method_not_allowed
```

- Rotta **blocking** per design: il timeout della sandbox garantisce che la
  connessione non resti aperta oltre l'epoch della run.
- Handler di riferimento: eco (come la CLI). Handler di dominio reale resta
  la via libreria (scenario B).
- Bind di default **127.0.0.1:8130** (loopback-only); `--bind 0.0.0.0:PORT`
  esplicito e documentato come atto deliberato. Nessuna auth nel v0.7:
  il README dichiara "esporre solo dietro proxy con auth".

### 2.3 Scenario D — prova reale

- `scripts/scenarios/scenario-d.sh`: avvia `wasmbox-http` (runner con
  `trap kill EXIT`), poi con `curl` (presente nei runner CI) colpisce:
  1) echo ok (atteso `ok:true`, output atteso);
  2) request malformata → `bad_request` (400);
  3) wasm invalido → `invalid_wasm` (200, `ok:false`);
  4) guest che esaurisce fuel (`fuel-bomb.wasm` con `limits.max_fuel`
     piccolo + `epoch_timeout_ms: null`) → `error:"fuel_exhausted"`,
  dimostrando che i limiti di risorsa viaggiano anche via HTTP.

---

## 3) Scenario E — FFI/C-ABI (crate separato `wasmbox-ffi`)

Perché: ROADMAP §2.9 opzione (c) — interoperabilità con Python/Node/Go/
Android/iOS via C-ABI stabile.

### 3.1 Crate

```
crates/wasmbox-ffi/       # crate-type = ["cdylib","staticlib"]
```
Zero nuova dipendenza: usa solo `wasmbox-core` + `std`. Divieti aumentati:
**niente thread** nel crate (il ticker di `SandboxEngine::new` resta l'unico
thread interno), **mai puntatori alla memoria del guest** fuori dal crate
(ogni `run` copia l'output in un buffer `malloc`-ato).

### 3.2 ABI C-stabile (minima)

```c
/* limits NULL → SandboxConfig::default() */
wasmbox_engine_t *wasmbox_engine_new(const uint8_t *wasm, size_t wasm_len,
                                     const wasmbox_limits_t *limits);
void wasmbox_engine_free(wasmbox_engine_t *engine);

/* una engine NON è thread-safe: documentato. L'output è allocato dal
   crate (malloc) e va liberato con wasmbox_buffer_free. */
wasmbox_status_t wasmbox_engine_run(wasmbox_engine_t *engine,
    const uint8_t *input, size_t input_len,
    uint8_t **out, size_t *out_len);
void wasmbox_buffer_free(uint8_t *buf, size_t len);

/* legale solo subito dopo una run fallita; NULL altrimenti */
const char *wasmbox_last_error(wasmbox_engine_t *engine);
/* codice exit stile CLI: 0 ok, 2..10 altrimenti */
uint32_t wasmbox_last_error_code(wasmbox_engine_t *engine);
```

`wasmbox_status_t`: 0 ok · 1 argomento invalido · 2 init fallito ·
3 run fallita (causa leggibile con `last_error`/`last_error_code`).
L'handler `ask` nella FFI è **eco** (v0.7): un callback C opzionale
richiederebbe una puntatore + invariante unsafe; rimandato a una v0.8
se serve davvero.

### 3.3 Prova reale

- Test Rust in-crate (chiamate `extern "C"` interne): status/exit per
  i casi già coperti dal core — InvalidWasm→9/invalid, MissingExport→8,
  FuelExhausted via fuel-bomb→3, round-trip echo→0.
- Script `scripts/scenarios/scenario-e.sh`: `python3` **stdlib** (`ctypes`)
  carica `target/release/libwasmbox_ffi.so` (linux), nuova engine + run
  echo + free buffer; FAIL se python manca (messaggio esplicito).
- CI: solo job linux (python3 è preinstallato su ubuntu-latest: zero
  dipendenze aggiunte). MACOS/WINDOWS: macOS ha python3 di default
  (verifica facoltativa); Windows NON viene collaudato in v0.7 (la cdylib
  .dll compila, ma niente script ctypes su PowerShell — dichiarato).

---

## 4) Scenario F — tool LLM reale via OpenRouter (estensione dello scenario B)

Natura: nel protocollo `tool:<nome>:<arg>` dello scenario B aggiunge il tool
`llm:<prompt>`. La chiamata di rete sta **interamente nell'host handler**
(crate `scenario-tool-handler`, feature opt-in) — mai nella sandbox, mai in
`wasmbox-core`.

### 4.1 Decisione utente richiesta PRIMA di implementare

- Richiede `OPENROUTER_API_KEY` impostata dall'utente in **Settings →
  Environment** (mai nel repo, mai nel guest, mai nella risposta del guest:
  solo `process.env`/`std::env` lato host la legge).
- **Ingress di rete lato host**: nuova classe di I/O. Il design lo permette
  (l'handler è libero), ma serve il consenso esplicito su questa spec.
- Limiti a difesa del portafoglio: il tool `llm` resta soggetto a
  `MAX_ASK_CALLS` (16/run), payload ≤ `max_ask_payload_bytes` della sandbox,
  prompt capito a 8 KiB lato handler, **timeout HTTP 30 s, retry 0**,
  modello default con suffisso `:free` (free tier).

### 4.2 Dettagli operativi (verificati sulle doc OpenRouter di ottobre 2026)

- Base URL: `https://openrouter.ai/api/v1` · `POST /chat/completions`
- Header: `Authorization: Bearer $OPENROUTER_API_KEY`,
  `Content-Type: application/json`; `HTTP-Referer` / `X-OpenRouter-Title`
  opzionali (non richiesti).
- Body: `{"model": MODEL, "messages":[{"role":"user","content":<prompt>}]}`
  → testo nella risposta a `choices[0].message.content`.
- Errori: body `{"error":{"code":<int>,"message":"…"}}`; 401 credenziali ·
  402 crediti · 429 rate limit · 5xx provider. Mappati in
  `HostError::Custom("llm:<http code>: <message>")` — che diventa
  `SandboxError::Host` → exit 7 in CLI (come il tool `boom` già fa).
- Client HTTP: `ureq 3.x` come **dipendenza optional** dietro feature
  `llm` del crate scenario-tool-handler
  (`default-features = false, features = ["rustls","json"]`).
  `wasmbox-core` non prende NESSUNA dipendenza nuova; con la feature OFF
  il tool `llm` risponde `tool_result:llm=(feature 'llm' non abilitata)`.
- Non-determinismo LLM gestito nel protocollo: la risposta del tool è
  `tool_result:llm=<testo del modello>`; il test della pipeline OTA
  (scenario F) controlla **che ci sia una risposta non vuota**, non il
  contenuto esatto (il modello è generativo). Il costo rimane visibile
  nel conteggio `ask_calls` post-run (metriche già presenti).

### 4.3 Fallback e test

- Runner `scripts/scenarios/scenario-f.sh`: **SKIP esplicito** con messaggio
  se `OPENROUTER_API_KEY` non è impostata (la suite resta sempre verde).
  Con la chiave: una run reale `tool:llm:rispondi "OK"` → output atteso
  `/^tool_result:llm=.+/`, più un caso 401 simulato (chiave sbagliata:
  `OPENROUTER_API_KEY=invalid` → atteso `SandboxError::Host` contenente
  `llm:401`), più il caso 429 rate-limit (documentato, non forzato).
- `OPENROUTER_MODEL` env variabile per scegliere il modello (default
  dichiarato nel README del crate; consultare `GET /api/v1/models` per i
  `:free` correnti — la rotazione dei modelli gratuiti è veloce, il codice
  NON hard-coda un modello dividendolo da env).

---

## 5) ORDINE DI IMPLEMENTAZIONE (dopo la conferma)

| # | Deliverable | Prova di accettazione |
|---|---|---|
| 1 | M1: fuel-deterministico via runner fuel-only | 3 run consecutive di scenario-c → fuel-bomb esce **sempre 3** nella run b) |
| 2 | Scenario D: `crates/wasmbox-http` + `scenario-d.sh` | 4 casi curl PASS (ok / bad_request / invalid_wasm / fuel_exhausted) |
| 3 | Scenario E: `crates/wasmbox-ffi` + `scenario-e.sh` | test in-crate verdi + run reale python3 ctypes exit 0 |
| 4 | Scenario F: feature `llm` + `scenario-f.sh` | skip senza chiave; run REALE (una volta impostata la chiave) exit 0 con `tool_result:llm=…` |
| 5 | Docs: `integration.md` (§ scenari D/E/F), `index.md`, `log.md`, `README.md`, ROADMAP (§2.9 aggiornata a "opzioni avviate/residue") | 0 link rotti |
| 6 | Baseline completa + push → CI/CodeQL verdi | check, test, clippy, fmt, guest build; gh run list |

Stima di carico: 1–2 sono i più grossi (server + protocollo JSON a mano);
3 è meccanico; 4 è piccolo ma con decisione utente sul settaggio chiave.

## 6) Cosa NON fa (escluso di default, dichiarato)

- release binarie di `wasmbox-http` in `release.yml` v0.7 (solo build/test CI);
- auth/TLS nel server HTTP (documentato: proxy esterno);
- streaming SSE OpenRouter (solo non-streaming);
- ogni nuova dipendenza IN `wasmbox-core` (resta wasmtime+thiserror);
- callback C a handler esterno nella FFI v0.7 (handler eco fisso);
- esporre la API key al guest: impossibile per architettura.

## 7) Punti che richiedono la TUA conferma esplicita

1. **M1**: runner fuel-only nel repo (nuovo mini-crate) per la run
   deterministica? (niente flag CLI)
2. **Scenario D (HTTP)**: crate `wasmbox-http` con bind loopback
   127.0.0.1:8130, rotta unica `POST /run`, curl nel runner. OK?
3. **Scenario E (FFI)**: crate `wasmbox-ffi` cdylib+staticlib, handler eco
   fisso, prova con python3 ctypes su linux; niente .dllWindows/`wasmbox_ffi`
   nei binari di release v0.7. OK?
4. **Scenario F (OpenRouter)**: CONSENSO per l'ingresso di rete lato host?
   E: imposti tu `OPENROUTER_API_KEY` (e opzionalmente `OPENROUTER_MODEL`)
   in Settings → Environment quando il codice è pronto? Il default
   proposto: timeout 30 s, retry 0, prompt ≤ 8 KiB, modello `:free` via env.
5. **Ordine**: procedo 1→6 come in §5, o preferisci un sottoinsieme
   (es. solo M1+F)?
