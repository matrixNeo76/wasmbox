---
type: Specification
title: "wasmbox — AGENT LOOP PLAN: sandbox token-efficient per LLM (v0.9, da approvare)"
description: "Spec/piano per il loop rapido LLM→wasmbox: come Manus, Claude Code e Codex risparmiano token nelle chat agentiche e come applicarlo qui — un solo tool, envelope compatto, server MCP stdio persistente su wasmbox-core. Nessun codice finché l'utente non conferma."
resource: "docs/agent-loop-plan.md"
tags: ["spec", "agenti", "llm", "token-efficiency", "mcp", "sandbox", "okf"]
updated: "2026-10-11"
---

# AGENT LOOP PLAN — sandbox token-efficient per LLM (v0.9, da approvare)

> **Regola di casa (come extension-plan.md)**: spec **proposta** — **nessun
> codice finché l'utente non conferma**. Sequenza: spec → conferma →
> implementazione → baseline verde → push → CI/CodeQL.
> **Invariante 0**: il core (`wasmbox-core`) resta intatto; tutto in un
> crate nuovo separato, come già fatto per `wasmbox-http`, `wasmbox-ffi`,
> `wasmbox-wasi`.

---

## 1) Perché: lo stato dell'arte (fact-check 2026-10-11)

Ricerca web su fonti primarie (URL completi in §7). Sintesi operativa:

| Attore | Tecnica chiave di risparmio token | Fonte (§7) |
|---|---|---|
| Manus | Prefisso di prompt stabile + **tool masking** (mai ridefinire la lista dei tool: invalida la KV-cache) + risultati voluminosi **su file-system** con solo path/URL in chat + errori nel contesto ma **senza log grezzi infiniti** + append-only | Manus blog, "Context Engineering for AI Agents" |
| Claude Code | Output dei tool limitato (Bash ~30k char, errori head+tail) + subagent con finestra di contesto dedicata + `CLAUDE.md` breve (≤200 righe) + compaction con sommario, non trascritto | code.claude.com docs + Anthropic engineering |
| Codex (OpenAI) | `apply_patch` (diff, non file interi) + risposta tool capata ~10k token (troncamento middle) + `AGENTS.md` breve + chiamate indipendenti in parallelo + compaction | developers.openai.com |
| Anthropic (MCP) | **Eseguire dentro la sandbox e al modello solo il risultato** (caso documentato: 150k token → 2k caricando i tool via codice e filtrando i dati) | Anthropic, "Code execution with MCP" |
| Framework sandbox 2026 | E2B / Daytona / Modal / LangChain Deep Agents: la sandbox serve **per eseguire, non per mostrare** — envelope `{ok, output, error}`, stdout capato, runtime persistente/warm-reuse, snapshot | docs e2b / daytona / modal / langchain |

**Schema condiviso rilevante per wasmbox**: la sandbox esiste per produrre
**un envelope piccolo e decisivo**, non per riversare transcript nel
contesto. Il costo del round-trip conta più del costo della risposta.

### Dove wasmbox si trova già bene

- Envelope già esiste: CLI `run --json` → `{"ok":true,"output":"…"}` o
  `{"ok":false,"error":"snake_case","message":"…"}` (skill `SKILL.md`,
  exit code 0..10 deterministici);
- limiti SEMPRE attivi e **misurati** (`benches/perf.rs`, artifact CI);
- errori già tipizzati in snake_case (`fuel_exhausted`, `timeout`,
  `guest_out_of_memory`, `ask_limit_exceeded`, `payload_too_large`, …);
- 4 vie d'uso già pronte: CLI / libreria / HTTP `/run` / FFI C-ABI.

### Cosa manca (gap dichiarato, v0.9)

1. **Un SOLO tool d'ingresso pensato per il loop agente**: oggi un host
   agentico deve scegliere tra 4 interfacce; il pattern SOTA espone
   *un tool unico* e diagnostica dentro l'envelope.
2. **Envelope senza i contatori che l'agente usa per decidere se
   ritentare o scartare**: `ask_calls`, `duration_ms`, `output_truncated`.
3. **Runtime non persistente**: ogni chiamata CLI paga setup dell'engine
   (compile freddo ~1.3–2.2 ms, cache-hit ~0.14–0.24 ms — numeri dei
   nostri bench). Il pattern e2b/Modal/Daytona 2026 è **warm reuse**:
   un processo vivente che tiene `Engine` + modulo già compilato
   (cache `.cwasm` per hash) e risponde alle chiamate successive in µs.
4. **Interfaccia MCP assente**: per i coding agent la via primaria
   resta shell+skills (punto già coperto dal repo con `SKILL.md` +
   CLI), ma MCP (`modelcontextprotocol.io`, spec `2026-07-28`) è lo
   standard strutturato che tutti gli host documentano **in aggiunta**
   — e l'unico canale per host agentici senza shell (Claude Desktop,
   LangGraph, agenti custom).

---

## 2) Progettazione: un solo tool, MCP stdio, envelope compatto

### 2.1 Struttura modulare (nessuna invasione del core)

```
crates/wasmbox-mcp/          # membro workspace, binario wasmbox-mcp
├── src/
│   ├── main.rs              # loop stdio (JSON-RPC 2.0 scritto a mano)
│   ├── protocol.rs          # subset MCP: initialize, tools/list, tools/call
│   ├── envelope.rs          # RunResult + cap/truncation (error compatti)
│   └── runner.rs            # engine persistente + cache .cwasm (modulo)
└── tests/
    └── mcp_test.rs          # round-trip JSON-RPC su pipe in-memory
```

Dipendenze: **solo `wasmbox-core` + std** (stessa disciplina del
workspace: niente tokio/axum/serde/rmcp). Il protocollo qui è un subset
di 3 metodi, realizzabile con il parsing JSON a mano già collaudato in
`wasmbox-http/src/json.rs`.

**Il core non vede alcun nuovo import; il crate è opzionale per chi non
usa MCP, identico a come `wasmbox-wasi` raggruppa `wasmtime-wasi`.**

### 2.2 Un solo tool: `wasmbox_run`

Schema MCP (per `tools/list`, non per l'LLM: l'host lo inietta):

```json
{
  "name": "wasmbox_run",
  "description": "Esegue un modulo WASM (guest) nella sandbox wasmbox
                  con limiti di risorsa SEMPRE attivi (fuel, timeout,
                  memoria, budget ask). Ritorna un envelope JSON
                  compatto {ok, output|error, ask_calls, duration_ms}.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "guest":           {"type": "string", "description": "modulo .wasm in base64, oppure path locale"},
      "input":           {"type": "string", "description": "input del guest (default: vuoto)"},
      "max_fuel":        {"type": "integer"},
      "timeout_ms":      {"type": "integer"},
      "output_bytes_max":{"type": "integer", "description": "cap output, default 4096"}
    },
    "required": ["guest"]
  }
}
```

**Regola chiave**: i limiti di default sono GIÀ sicuri nel core (fuel
1e9, timeout 1000 ms, memoria 16 MiB, ask 1024 chiamate / 1 MiB
payload). L'LLM NON li specifica di default; l'agente può solo
RESTRINGERE il budget, mai allargarlo oltre i massimi del core
(`
max_memory_bytes` resta il tetto fisico). Niente `unlimited`: il tool
schema è piccolo e chiuso.

**Primaria vs complementare (correzione post-revisione dell'utente,
verificata sui documenti dei vendor — §7)**: la via **primaria** per i
coding agent resta quella che il repo ha GIÀ: **skills + CLI + envelope
JSON** (`SKILL.md` + `wasmbox-cli run --json`, exit code 0..10). Claude
Code, Codex CLI, Cursor e Windsurf/Cascade hanno tutti un tool di shell
con sandbox propria e usano skill per guidare l'uso della CLI; tutti
documentano MCP come integrazione strutturata **in aggiunta** (non in
sostituzione). Il crate MCP serve agli host **senza shell** o desk-side
(Claude Desktop, LangGraph, agenti custom): è il secondo canale, non il
primo. Niente mirror di `/run` HTTP né basket di tool — MCP espone
un solo tool e la CLI resta l'interfaccia shell.

### 2.3 Envelope: il cuore del risparmio token

Uniforme con la CLI (stessi snake_case) ma con i contatori in più:

```json
// ok:
{"ok":true,"output":"echo_result:HI","ask_calls":1,"duration_ms":2}
// fallimento:
{"ok":false,"error":"fuel_exhausted","message":"budget 1000000","duration_ms":500}
```

**Regole di compattazione (misurabili + vincoli espliciti)**:

1. `output` **capato a 4 KiB di default** (`output_bytes_max` fino a
   1 MiB max): oltre → `output_truncated: true` + envelope con solo
   prefisso+suffix (head+tail, come fa Claude Code con Bash). Il guest
   non è un log: è un risultato.
2. `message` **capato a 256 byte**: niente backtrace completi.
3. `duration_ms` copre solo instantiate+call (no compile); tempo di
   compile separato solo con opt-in `with_timing`.
4. Campi e ordine fissi: `(ok, error, message, output, ask_calls,
   duration_ms, output_truncated)`. Stabile tra versioni
   (KV-cache friendly, lezione Manus).

**Perché così**: gli agenti SOTA non riversano lo stdout del runner
nel contesto: capano e riportano l'outcome deciso. Con l'envelope
capato, decine di round-trip agentic costano meno di una singola
risposta estesa.

### 2.4 Protocollo MCP (subset) su stdio

```
→ {"jsonrpc":"2.0","id":1,"method":"initialize","params":{...}}
← {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2026-07-28",
     "capabilities":{"tools":{}},"serverInfo":{"name":"wasmbox-mcp",
     "version":"0.1.0"}}}
→ notification initialized
→ {"jsonrpc":"2.0","id":2,"method":"tools/list"}
← tools/list → [wasmbox_run (inputSchema §2.2)]
→ {"jsonrpc":"2.0","id":3,"method":"tools/call",
   "params":{"name":"wasmbox_run","arguments":{...}}}
← {"jsonrpc":"2.0","id":3,"result":{"content":[
     {"type":"text","text":"<envelope JSON §2.3>"}]}}
```

- **Loop stdio bloccante, un req alla volta**: read → parse → dispatch →
  write. Log SU STDERR SOLO (stdout = protocollo, mai altro).
- Risultato `content` = **una sola option text con l'envelope**: niente
  doppia serializzazione JSON + stdout.
- Versione protocollo dichiarata: **`2026-07-28`** (la spec corrente al
  fact-check: approccio STDIO+Streamable HTTP; la vecchia transport
  HTTP+SSE del 2024-11-05 è formalmente deprecata). Subset stdio-only:
  il loop billing è blindato rispetto alle differenze di transport.
  Campi extra dei client → ignorati pacificamente; metodo sconosciuto
  → error JSON-RPC `method_not_found` (senza panic).
- Cache moduli compilati per hash (come la `.cwasm` del core) nel
  processo MCP persistente: compile una volta, run sempre.

### 2.5 Valutazione di altri protocolli (post-revisione utente)

Richiesta esplicita: valutare A2A, SSE e protocolli di altro tipo per
rafforzare le connessioni al core. Verdetto per-fonte (URL in §7):

| Protocollo | Cos'è davvero (2026) | Applicabilità a wasmbox |
|---|---|---|
| **A2A** (Google → Linux Foundation, **v1.0** prod-ready) | Protocollo **agent↔agent**: Discovery, task delegation, coordinamento; binding JSON-RPC/gRPC/HTTP. NON è un'API di tool/sandbox | NON applicabile: `wasmbox` non è un agente. Solo se/parche ESPOSTO via agente wrapper (out-of-scope). Monitoraggio |
| **SSE** | Non è un protocollo di tool-calling: transport deprecato della vecchia MCP HTTP+SSE (2025-03-26 deprecato, rimane come stream opzionale in Streamable HTTP). Usato per streaming di task in A2A | NON applicabile alla v0.9 (stdio). Per il REMOTO è lo Streamable HTTP di MCP a dominare, v1.0 se richiesta |
| **ACP** (IBM/BeeAI) | Assorbito IN A2A sotto la Linux Foundation: non è più un protocollo indipendente | Nessuna azione |
| **AGNTCY / LMOS** | Infrastruttura multi-agente (discovery OASF, messaggistica SLIM) e interoperabilità federata; LMOS dichiarato "work in progress", NON W3C | NON applicabile: monitoraggio (per sistemi federati di agenti, out-of-scope) |
| **MCP Streamable HTTP** | La transport REMOTA standard di MCP (2026-07-28: POST JSON o stream SSE per richiesta; subs lato server per notifiche) | APPLICABILE a v1.0: stesso subset protocol, transport HTTP opzionale. Out of scope per la v0.9 |

**Verdetto operativo**: per una sandbox locale a UNA capability
(`run_guest`) — (a) la via **skills+CLI+JSON** già esistente resta il
canale universale; (b) un **MCP stdio** add-on copre gli host senza
shell; (c) **Streamable HTTP** (MCP remoto) è l'unico protocollo da
aggiungere davvero dopo, SOLO se serve accesso remoto/multi-tenant
(già coperto in parte da `wasmbox-http` "/run" per orchestratori);
(d) A2A/SSE/ACP/AGNTCY non rafforzano la connessione al core perché
operano AL DI SOTTO di questa singola capability.

### 2.6 Invarianze

- `crates/wasmbox-core/` **INADIVERTITO** (verify grep: 0 nuovi
  import; come già verificato dopo v0.7 e v0.8);
- `wasmbox-cli` invariato (rimane l'interfaccia umana/shell);
- 4 moduli in `crates/wasmbox-mcp` (§2.1) + 1 file test.

---

## 3) Prova finale (test nel crate `wasmbox-mcp`)

Round-trip MCP reale su pipe in-memory, `tests/mcp_test.rs`:

1. `initialize` → risposta con protocolVersion;
2. `tools/list` → esiste ESATTAMENTE un tool (`wasmbox_run`);
3. `tools/call` con guest echo
   (`target/wasm32-unknown-unknown/release/guest_echo.wasm`, come nei
   test CI — niente WAT inline duplicati) → envelope
   `ok:true`, `ask_calls:1`, `duration_ms` numerico;
4. fuel-bomba con `max_fuel` basso → `error: "fuel_exhausted"`
   (determinismo come M1/extension-plan);
5. guest che risponde >4 KiB → `output_truncated:true` + head+tail;
6. **test envelope-size** (misurato, non stimato): byte dell'envelope
   tipico < soglia fissa (guard anti-bloat: la CI si rompe se un
   refactor allarga l'envelope).

---

## 4) Sequenza di implementazione (dopo l'approvazione)

| # | Passo | Comando di prova |
|---|---|---|
| 1 | Nuovo crate `crates/wasmbox-mcp` (4 moduli §2.1) | `cargo check --workspace` |
| 2 | Round-trip test §3 (6 step) | `cargo test -p wasmbox-mcp` |
| 3 | Docs: scenarios.md + README + index/log/ROADMAP | — |
| 4 | Baseline completa | `cargo test --workspace` (47 → ~53) |
| 5 | commit + push + CI/CodeQL | `gh run list --limit 2` |

---

## 5) Rischi e mitigazioni (onesti)

| Rischio | Mitigazione |
|---|---|
| JSON-RPC MCP con parser a mano fragile | Subset rigoroso: 3 metodi + notifica `initialized`; test round-trip; errori gestiti (mai panic); stdout solo protocollo |
| Client che manda campi extra / spec alla deriva | Campi extra ignorati; metodo sconosciuto → `method_not_found`; versione dichiarata in `tools/list` |
| Envelope che cresce silenziosamente | Test envelope-size con soglia fissa (§3) |
| Processo persistente trattiene risorse | `SandboxEngine` già `Drop`-safe (ticker join); opzione `--idle-exit SEC` |
| Divergenza di versione MCP | Dichiarare `2026-07-28`; subset minimo = superficie di compatibilità minima |
| Sicurezza (guest ostile) | IDENTICA al core: fuel+epoch+StoreLimits; il layer MCP NON aggiunge capacità (WASI out-of-scope qui; già gestita da `wasmbox-wasi` se_host opt-in) |

---

## 6) In/out of scope (v0.9)

**In scope**: crate `wasmbox-mcp` stdio (3 metodi), un solo tool,
envelope capato con sentinel (§2.3), test §3, docs
(scenarios.md, README, index/log/ROADMAP), baseline verde.

**Out of scope (dichiarato)**: MCP **Streamable HTTP** (v1.0 se serve
accesso remoto; già coperto parzialmente da `wasmbox-http`);
protocolli agent↔agent — A2A v1.0 (Linux Foundation), ACP (assorbito in
A2A), AGNTCY/LMOS (infrastruttura federata, non W3C) — non applicabili
a una sandbox a capability singola, solo monitoraggio; WASI nel tool
MCP (v1.0, opt-in); parser JSON completo (solo subset); snapshotting
stile MicroVM; LLM routing già coperto da scenario F
(v0.7, feature `llm` opt-in), non duplicato qui.

---

## 7) Fonti (fact-check 2026-10-11)

- Manus — "Context Engineering for AI Agents" (2025-07-18):
  <https://manus.im/blog/Context-Engineering-for-AI-Agents-Lessons-from-Building-Manus>
- Anthropic — "Effective context engineering for AI agents":
  <https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents>
- Anthropic — "Writing effective tools for agents":
  <https://www.anthropic.com/engineering/writing-tools-for-agents>
- Anthropic — "Code execution with MCP":
  <https://www.anthropic.com/engineering/code-execution-with-mcp>
- Claude Code docs — context-window / memory / sub-agents / tools-reference
  (Bash output truncation ~30k char):
  <https://code.claude.com/docs/en/context-window>,
  <https://code.claude.com/docs/en/memory>,
  <https://code.claude.com/docs/en/sub-agents>,
  <https://code.claude.com/docs/en/tools-reference>
- OpenAI — Codex best practices / developer-commands / config-reference
  (`/compact`, `model_context_window`):
  <https://developers.openai.com/codex/learn/best-practices>,
  <https://developers.openai.com/codex/developer-commands>,
  <https://developers.openai.com/codex/config-reference>
- OpenAI — Codex prompting guide (tool cap ~10k token, troncamento
  middle, apply_patch, indipendenti in parallelo):
  <https://developers.openai.com/cookbook/examples/gpt-5/codex_prompting_guide>
- OpenAI — blog "Run long-horizon tasks with Codex" (2026-02) +
  compaction guide + skills/shell tips:
  <https://developers.openai.com/blog/run-long-horizon-tasks-with-codex>,
  <https://developers.openai.com/api/docs/guides/compaction>,
  <https://developers.openai.com/blog/skills-shell-tips>
- LangChain — Deep Agents sandboxes (Runloop/Daytona/Modal):
  <https://www.langchain.com/blog/execute-code-with-sandboxes-for-deepagents>
- LangChain sandbox (Pyodide, unmaintained):
  <https://github.com/langchain-ai/langchain-sandbox>
- OpenAI Code Interpreter / Containers:
  <https://developers.openai.com/api/docs/guides/tools-code-interpreter>
- E2B (SDK + connect LLM; Embed su Firecracker):
  <https://docs.e2b.dev/quickstart/connect-llms>,
  <https://e2b.dev/resources/introducing-e2b-embed>
- Daytona (REST + code-run + snapshot):
  <https://www.daytona.io/docs/en/sandboxes/>,
  <https://www.daytona.io/docs/en/process-code-execution/>
- Modal Sandboxes (exec, stream, snapshot):
  <https://modal.com/docs/guide/sandboxes>,
  <https://modal.com/docs/guide/sandbox-spawn>
- MCP — transport stdio/Streamable HTTP + tools + structuredContent,
  spec corrente **2026-07-28** (Streamable HTTP one-endpoint + SSE
  per-request deprecate HTTP+SSE 2024-11-05):
  <https://modelcontextprotocol.io/specification/2026-07-28/basic/transports>,
  <https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http>,
  release notes: <https://blog.modelcontextprotocol.io/posts/2026-07-28/>;
  spec 2025-06-18 (compat): <https://modelcontextprotocol.io/specification/2025-06-18/basic/transports>
- A2A — spec v1.0 (agent↔agent; binding JSON-RPC/gRPC/HTTP;
  Linux Foundation, 150+ organizzazioni):
  <https://a2a-protocol.org/latest/specification/>,
  <https://a2a-protocol.org/latest/blog/2026/03/12/a2a-protocol-ships-v10-production-ready-standard-for-agent-to-agent-communication/>,
  <https://www.linuxfoundation.org/press/a2a-protocol-surpasses-150-organizations-lands-in-major-cloud-platforms-and-sees-enterprise-production-use-in-first-year>
- ACP (IBM/BeeAI) — assorbito in A2A: <https://github.com/i-am-bee/acp>
- AGNTCY (Linux Foundation, OASF/SLIM): <https://docs.agntcy.org/>,
  <https://www.linuxfoundation.org/press/linux-foundation-welcomes-the-agntcy-project-to-standardize-open-multi-agent-system-infrastructure-and-break-down-ai-agent-silos>
- LMOS (Eclipse, work-in-progress dichiarato, non W3C):
  <https://eclipse.dev/lmos/docs/lmos_protocol/introduction/>
- Vendor docs MCP/coding-agent (evidenza che shell+skills è la via
  primaria, MCP quella strutturata aggiuntiva):
  Claude Code sandboxing+skills <https://code.claude.com/docs/en/sandboxing>,
  <https://code.claude.com/docs/en/skills>;
  Codex CLI sandboxing/skills/MCP <https://learn.chatgpt.com/docs/sandboxing>,
  <https://learn.chatgpt.com/docs/build-skills>,
  <https://learn.chatgpt.com/docs/extend/mcp?surface=cli>;
  Cursor skills/MCP <https://cursor.com/docs/skills>,
  <https://cursor.com/docs/mcp>;
  Windsurf/Cascade MCP <https://docs.devin.ai/windsurf/plugins/cascade/mcp>;
  Daytona MCP server <https://www.daytona.io/docs/en/mcp/>
- Wassette (Microsoft: MCP ↔ Wasmtime, capability deny-by-default):
  <https://microsoft.github.io/wassette/latest/concepts.html>
- Wasmer SDK 2026 (sandbox Wasm in-process, ~0.1 ms claim del progetto):
  <https://wasmer.io/posts/wasmer-local-sandboxes-for-ai-agents>
