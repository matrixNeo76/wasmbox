"""Renderizza il report dei test in HTML (letto da env: STATUS, REPORT, DURATION).

Extra (2026-10-09): se il preview ha anche eseguito CLI e UI demo,
li mostra inline: il comando da riprodurre e il suo output (env
CLI_DEMO), più lo screenshot Slint headless della UI (file BMP puntato
da UI_SCREENSHOT_BMP, embeddato base64 — l'env var non regge 300 KB).
"""
import base64
import html
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

  <pre>{body}</pre>

  <footer>
    Workspace: <code>crates/wasmbox-core</code> + <code>crates/wasmbox-cli</code> + <code>crates/wasmbox-ui</code> + <code>examples/guest-echo</code><br>
    Skill agenti: <code>skills/wasmbox/SKILL.md</code> &middot; Spec: <code>docs/blueprint.md</code>
  </footer>
</div>
</body>
</html>""")
