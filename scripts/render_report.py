"""Renderizza il report dei test in HTML (letto da env: STATUS, REPORT, DURATION).

Extra (2026-10-09): se il preview ha anche eseguito CLI e UI demo,
li mostra inline: il comando da riprodurre e il suo output (env
CLI_DEMO), più lo screenshot Slint headless della UI (file BMP puntato
da UI_SCREENSHOT_BMP, embeddato base64 — l'env var non regge 300 KB).

Extra (2026-10-10): mappa dei concetti OKF come grafo interattivo
(vis-network via CDN). I nodi sono i file markdown del bundle
(concept + riservati index/log + file di fruzione fuori da docs/);
gli archi sono i link markdown realmente presenti nei file, scansionati
al momento del render: zero drift, il grafo è sempre fresco col repo.
"""
import base64
import html
import json
import os
import pathlib
import re

status = os.environ.get("STATUS", "UNKNOWN")
report_path = os.environ.get("REPORT", "")
duration = os.environ.get("DURATION", "n/a")
cli_demo = os.environ.get("CLI_DEMO", "")
ui_bmp_path = os.environ.get("UI_SCREENSHOT_BMP", "")
ui_b64 = ""
if ui_bmp_path and pathlib.Path(ui_bmp_path).exists():
    ui_b64 = base64.b64encode(pathlib.Path(ui_bmp_path).read_bytes()).decode("ascii")

raw = ""
if report_path and pathlib.Path(report_path).exists():
    raw = pathlib.Path(report_path).read_text(encoding="utf-8", errors="replace")

# Estrae i riepiloghi "test result: ok. N passed; M failed"
summaries = re.findall(r"^test result:.*$", raw, flags=re.M)
passed = sum(int(m) for m in re.findall(r"(\d+) passed", " ".join(summaries)))
failed = sum(int(m) for m in re.findall(r"(\d+) failed", " ".join(summaries)))

colors = {
    "PASS": ("#3ecf8e", "TEST PASS"),
    "FAIL": ("#ff6b6b", "TEST FAIL"),
    "SKIP": ("#f0c674", "TOOLCHAIN ASSENTE"),
}
color, label = colors.get(status, ("#8899aa", status))

body = html.escape(raw)

# --- Sezione demo (CLI live + UI screenshot) — generata solo se presenti ---
def demo_html() -> str:
    pre = html.escape(cli_demo)
    # L'immagine va copiata accanto a index.html e referenziata dal nome:
    # inline base64 ingrossa la pagina di ~300 KB e alcuni viewer la tagliano.
    img = (
        '<img src="ui-screenshot.bmp" width="320" height="240" '
        'alt="screenshot della UI Slint headless di wasmbox-ui">'
        if ui_b64
        else "<p>screenshot UI non disponibile</p>"
    )
    return f"""
  <h2>Prova la sandbox ora (CLI live, comandi copia-incolla)</h2>
  <p class="sub">bash dentro il preview: esegue wasmbox-cli sul guest reale — la stessa skill usata dagli agenti AI.</p>
  <pre style="user-select:all">cargo run -p wasmbox-cli -- run target/wasm32-unknown-unknown/release/guest_echo.wasm "ciao" --json</pre>
  <h2>Output dell'ultima demo CLI eseguita dal preview</h2>
  <pre>{pre}</pre>
  <h2>UI Slint (screenshot headless, renderizzato dal SoftwareRenderer)</h2>
  {img}
  <p class="sub">Verde = ultima run guest ok, rossa = fallita. Immagine separata: <a href="ui-screenshot.bmp" download>ui-screenshot.bmp</a>. Generato da <code>wasmbox-ui --screenshot</code>.</p>
"""

demo = demo_html()


# --- Mappa dei concetti OKF: grafo statico con vis-network (CDN) ---
# Nodi = file markdown rilevanti; archi = link markdown tra loro, scansionati
# ora dal repo. Funzione separata e riusabile (futura emissione standalone).
TYPE_SHAPE = {  # type del frontmatter (o family per i riservati)
    "Specification": "#8ec7ff",
    "Reference": "#f0c674",
    "Collection": "#3ecf8e",
    "Log": "#b48ead",
}

def frontmatter_type(text: str) -> str:
    m = re.match(r"^---\n(.*?)\n---\n", text, re.S)
    if m:
        t = re.search(r"^type:\s*\"?([A-Za-z ]+)\"?", m.group(1), re.M)
        if t:
            return t.group(1).strip()
    return "Fuori bundle"

def build_okf_graph(root: pathlib.Path) -> dict:
    nodes: dict[str, dict] = {}
    edges: list[dict] = []

    # Nodi candidati: bundle docs/ + file di fruzione rilevanti.
    candidates: list[tuple[str, pathlib.Path, str]] = []
    for p in sorted((root / "docs").glob("*.md")):
        candidates.append((f"docs/{p.name}", p, "docs"))
    extras = [
        ("README.md", root / "README.md"),
        ("AGENTS.md", root / "AGENTS.md"),
        ("skills/wasmbox/SKILL.md", root / "skills/wasmbox/SKILL.md"),
    ]
    for name, p in extras:
        if p.exists():
            candidates.append((name, p, "root"))

    # Pass 1: registra TUTTI i nodi (serve perché un link da docs/ verso
    # un file fuori bundle come ../AGENTS.md trovi il nodo destinazione).
    types: dict[str, str] = {}
    for node_id, path, family in candidates:
        text = path.read_text(encoding="utf-8", errors="replace")
        ftype = frontmatter_type(text)
        types[node_id] = ftype
        color = TYPE_SHAPE.get(ftype, "#8899aa")
        shape = "dot"
        if node_id in ("docs/index.md", "docs/log.md"):
            shape = "diamond"  # riservati per convenzione OKF
        nodes[node_id] = {
            "id": node_id,
            "label": node_id,
            "title": html.escape(ftype),
            "color": color,
            "shape": shape,
        }

    # Pass 2: scansiona i link markdown dai corpi e crea gli archi
    # (doppioni di esatta fonte+destino deduplicati: un link ripetuto due
    # volte nello stesso file è un solo collegamento nel grafo).
    seen_edges: set[tuple[str, str]] = set()
    edges: list[dict] = []
    for node_id, path, family in candidates:
        text = path.read_text(encoding="utf-8", errors="replace")
        body_text = re.sub(r"^---\n.*?\n---\n", "", text, count=1, flags=re.S)
        for m in re.finditer(r"\[[^\]]*\]\((?!#|http)([^)\s]+)", body_text):
            target = m.group(1)
            target = re.sub(r"[#].*$", "", target).strip()
            if not target or not target.endswith(".md"):
                continue
            resolved = (path.parent / target).resolve()
            try:
                rel = str(resolved.relative_to(root))
            except ValueError:
                continue
            if rel in nodes and rel != node_id:
                key = (node_id, rel)
                if key not in seen_edges:
                    seen_edges.add(key)
                    edges.append({"from": node_id, "to": rel})

    return {"nodes": list(nodes.values()), "edges": edges}


graph = build_okf_graph(pathlib.Path(".").resolve())
graph_json = json.dumps(graph, ensure_ascii=False, separators=(",", ":"))

graph_section = f"""
  <h2>Mappa dei concetti OKF v0.2 (grafo dei link reali)</h2>
  <p class="sub">Nodi = file markdown del bundle (colore = <code>type</code> del frontmatter; rombo = riservati index/log). Archi = link markdown realmente presenti nei file, scansionati al momento del render: sempre freschi col repo. Clic su un nodo per vederne il titolo.</p>
  <div id="okf-graph" style="width:100%;height:480px;border:1px solid #1e2733;border-radius:12px;background:#121821"></div>
"""

graph_js = f"""
<script src="https://unpkg.com/vis-network/standalone/umd/vis-network.min.js"></script>
<script>
(function () {{
  var g = {graph_json};
  var nodes = new vis.DataSet(g.nodes);
  var edges = new vis.DataSet(g.edges);
  new vis.Network(document.getElementById("okf-graph"), {{ nodes: nodes, edges: edges }}, {{
    autoResize: true,
    physics: {{ solver: "forceAtlas2Based", forceAtlas2Based: {{ gravitationalConstant: -60, springLength: 110 }} , stabilization: true }},
    interaction: {{ hover: true, tooltipDelay: 120 }},
  }});
}})();
</script>
"""


print(f"""<!doctype html>
<html lang="it">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>wasmbox — test report</title>
<style>
  :root {{ color-scheme: dark; }}
  * {{ box-sizing: border-box; }}
  body {{
    margin: 0; background: #0b0f14; color: #d7e0ea;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    min-height: 100vh;
  }}
  .wrap {{ max-width: 960px; margin: 0 auto; padding: 48px 24px; }}
  .brand {{ font-size: 13px; letter-spacing: .28em; text-transform: uppercase; color: #5b6b7c; }}
  h1 {{ font-size: 40px; margin: 8px 0 4px; color: #eef4fa; font-weight: 700; }}
  .sub {{ color: #7c8b9c; font-size: 14px; margin-bottom: 32px; }}
  .badge {{
    display: inline-block; padding: 8px 18px; border-radius: 999px;
    background: {color}1a; color: {color}; border: 1px solid {color}66;
    font-weight: 700; letter-spacing: .12em; font-size: 13px;
  }}
  h2 {{ color: #eef4fa; }}
  img {{
    border: 1px solid #1e2733; border-radius: 12px; display: block; margin: 10px 0;
  }}
  pre {{
    background: #121821; border: 1px solid #1e2733; border-radius: 12px;
    padding: 20px; overflow-x: auto; font-size: 12.5px; line-height: 1.55;
    color: #9fb0c2; white-space: pre-wrap; word-break: break-word;
  }}
  footer {{ margin-top: 28px; color: #46566a; font-size: 12px; line-height: 1.7; }}
  code {{ color: #8ec7ff; }}
  .stats {{
    display: flex; gap: 28px; margin: 22px 0 30px;
  }}
  .stat {{
    background: #121821; border: 1px solid #1e2733; border-radius: 12px;
    padding: 16px 22px; min-width: 120px;
  }}
  .stat .n {{ font-size: 26px; font-weight: 700; color: #eef4fa; }}
  .stat .l {{ color: #7c8b9c; font-size: 12px; margin-top: 4px; }}
</style>
</head>
<body>
<div class="wrap">
  <div class="brand">wasmbox &middot; sandbox wasm</div>
  <h1>Report di validazione</h1>
  <div class="sub">SandboxEngine &middot; Wasmtime 49 &middot; suite <code>cargo test -p wasmbox-core</code></div>

  <span class="badge">{label}</span>

  <div class="stats">
    <div class="stat"><div class="n">{passed}</div><div class="l">test passati</div></div>
    <div class="stat"><div class="n">{failed}</div><div class="l">test falliti</div></div>
    <div class="stat"><div class="n">{html.escape(duration)}</div><div class="l">durata suite</div></div>
  </div>

  {demo}

  {graph_section}
</div>

{graph_js}

  <pre>{body}</pre>

  <footer>
    Workspace: <code>crates/wasmbox-core</code> + <code>crates/wasmbox-cli</code> + <code>crates/wasmbox-ui</code> + <code>examples/guest-echo</code><br>
    Skill agenti: <code>skills/wasmbox/SKILL.md</code> &middot; Spec: <code>docs/blueprint.md</code>
  </footer>
</body>
</html>""")
