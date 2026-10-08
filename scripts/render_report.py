"""Renderizza il report dei test in HTML (letto da env: STATUS, REPORT, DURATION)."""
import html
import os
import pathlib
import re

status = os.environ.get("STATUS", "UNKNOWN")
report_path = os.environ.get("REPORT", "")
duration = os.environ.get("DURATION", "n/a")

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
  .stats {{ display: flex; gap: 16px; margin: 28px 0; flex-wrap: wrap; }}
  .stat {{
    flex: 1 1 160px; background: #121821; border: 1px solid #1e2733;
    border-radius: 12px; padding: 18px 20px;
  }}
  .stat .n {{ font-size: 30px; font-weight: 700; color: #eef4fa; }}
  .stat .l {{ font-size: 11px; letter-spacing: .18em; text-transform: uppercase; color: #5b6b7c; margin-top: 4px; }}
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
  <div class="sub">SandboxEngine &middot; Wasmtime 28 &middot; suite <code>cargo test -p wasmbox-core</code></div>

  <span class="badge">{label}</span>

  <div class="stats">
    <div class="stat"><div class="n">{passed}</div><div class="l">test passati</div></div>
    <div class="stat"><div class="n">{failed}</div><div class="l">test falliti</div></div>
    <div class="stat"><div class="n">{html.escape(duration)}</div><div class="l">durata suite</div></div>
  </div>

  <pre>{body}</pre>

  <footer>
    Workspace: <code>crates/wasmbox-core</code> + <code>examples/guest-echo</code><br>
    Spec: <code>docs/blueprint.md</code> &middot; validazione: check, clippy -D warnings, fmt, wasm32 build
  </footer>
</div>
</body>
</html>""")
