#!/usr/bin/env python3
"""Push dei file tracciati del workspace a un repo GitHub via REST API ufficiale.

Legge il Personal Access Token da stdin: il valore non viene mai stampato
né passato come argomento (invisibile in `ps`).

Uso:
    printf '%s' "<token>" | python3 scripts/push_via_api.py [owner/repo] [branch]

Default: matrixNeo76/wasmbox su branch main.
Crea blob + tree + commit + ref — nessun comando git, nessun pannello.
"""
import base64
import json
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request

API = "https://api.github.com"


def req(method, path, payload=None, token=None):
    data = json.dumps(payload).encode() if payload is not None else None
    r = urllib.request.Request(API + path, data=data, method=method)
    r.add_header("Accept", "application/vnd.github+json")
    r.add_header("X-GitHub-Api-Version", "2022-11-28")
    r.add_header("Content-Type", "application/json")
    if token:
        r.add_header("Authorization", "Bearer " + token)
    try:
        with urllib.request.urlopen(r, timeout=30) as resp:
            body = resp.read().decode() or "{}"
            return resp.status, json.loads(body)
    except urllib.error.HTTPError as e:
        raw = e.read().decode(errors="replace")
        try:
            return e.code, json.loads(raw)
        except Exception:
            return e.code, {"message": raw[:300]}


def die(msg, code=1):
    print(f"ERRORE: {msg}", file=sys.stderr)
    sys.exit(code)


def tracked_files():
    out = subprocess.run(
        ["git", "ls-files", "-z"], capture_output=True, check=True
    ).stdout
    return [f.decode() for f in out.split(b"\0") if f]


def main():
    token = sys.stdin.readline().strip()
    if not token:
        die("token vuoto su stdin — incolla il PAT nel messaggio di chat")

    target = sys.argv[1] if len(sys.argv) > 1 else "matrixNeo76/wasmbox"
    branch = sys.argv[2] if len(sys.argv) > 2 else "main"
    owner_repo = f"/repos/{target}"

    # 0. Autenticazione + accesso al repo (visibile anche se privato).
    st, me = req("GET", "/user", token=token)
    if st != 200:
        die(f"token non valido o revocato (HTTP {st}: {me.get('message')})")
    print(f"autenticato come: {me.get('login')}")

    st, repo = req("GET", owner_repo, token=token)
    if st != 200:
        die(f"repo {target} non accessibile (HTTP {st}: {me_msg(repo)})")
    print(f"repo: {target} | privato={repo.get('private')} | default={repo.get('default_branch')}")

    # 1. Head della branch (se esiste).
    files = tracked_files()
    total_files = len(files)
    print(f"file tracciati: {total_files}")
    st, ref = req("GET", f"{owner_repo}/git/ref/heads/{branch}", token=token)
    head_sha = ref.get("object", {}).get("sha") if st == 200 else None
    parent_tree = None
    if head_sha:
        st, head_commit = req(
            "GET", f"{owner_repo}/git/commits/{head_sha}", token=token
        )
        if st != 200:
            die(f"lettura commit head fallita (HTTP {st})")
        parent_tree = head_commit["tree"]["sha"]
        print(f"branch {branch}: esiste, head={head_sha[:7]} — commit aggiuntivo")
    else:
        # Il Git database API rifiuta i blob su repo privo di commit
        # (HTTP 409 "Git Repository is empty."): si crea il primo file —
        # e quindi la branch — con la Contents API, poi si prosegue con
        # blob/tree/commit per il resto.
        print(f"branch {branch}: repo vuoto — seed del primo file via Contents API")
        first = files[0]
        with open(first, "rb") as fh:
            content = base64.b64encode(fh.read()).decode()
        st, r = req(
            "PUT",
            f"{owner_repo}/contents/{urllib.parse.quote(first, safe='/')}",
            {
                "message": f"wasmbox: aggiungi {first}",
                "content": content,
                "branch": branch,
            },
            token=token,
        )
        if st not in (200, 201):
            die(f"seed di {first} fallito (HTTP {st}: {r.get('message')})")
        print(f"seed OK: {first}")
        st, ref = req("GET", f"{owner_repo}/git/ref/heads/{branch}", token=token)
        if st != 200:
            die(f"lettura ref dopo seed fallita (HTTP {st})")
        head_sha = ref["object"]["sha"]
        st, head_commit = req(
            "GET", f"{owner_repo}/git/commits/{head_sha}", token=token
        )
        if st != 200:
            die(f"lettura commit dopo seed fallita (HTTP {st})")
        parent_tree = head_commit["tree"]["sha"]
        files = files[1:]

    # 2. Blob per ogni file tracciato (solo sorgenti, niente target/).
    entries = []
    for i, path in enumerate(files, 1):
        with open(path, "rb") as fh:
            content = base64.b64encode(fh.read()).decode()
        st, blob = req(
            "POST",
            f"{owner_repo}/git/blobs",
            {"content": content, "encoding": "base64"},
            token=token,
        )
        if st != 201:
            die(f"blob {path} fallito (HTTP {st}: {blob.get('message')})")
        entries.append(
            {"path": path, "mode": "100644", "type": "blob", "sha": blob["sha"]}
        )
        if i % 5 == 0 or i == len(files):
            print(f"  blob {i}/{len(files)}")

    # 3. Tree (radice, oppure rispetto alla head esistente).
    tree_payload = {"tree": entries}
    if parent_tree:
        tree_payload["base_tree"] = parent_tree
    st, tree = req("POST", f"{owner_repo}/git/trees", tree_payload, token=token)
    if st != 201:
        die(f"tree fallito (HTTP {st}: {tree.get('message')})")

    # 4. Commit.
    commit_payload = {
        "message": (
            "wasmbox: sandbox WebAssembly isolata con Wasmtime 28\n\n"
            "Workspace Rust (wasmbox-core + guest-echo), blueprint in "
            "docs/, test e2e e integrazione GitHub Advanced Security."
        ),
        "tree": tree["sha"],
        "parents": [head_sha] if head_sha else [],
    }
    st, commit = req("POST", f"{owner_repo}/git/commits", commit_payload, token=token)
    if st != 201:
        die(f"commit fallito (HTTP {st}: {commit.get('message')})")
    sha = commit["sha"]
    print(f"commit creato: {sha[:7]}")

    # 5. Ref della branch.
    if head_sha:
        st, r = req(
            "PATCH",
            f"{owner_repo}/git/refs/heads/{branch}",
            {"sha": sha},
            token=token,
        )
    else:
        st, r = req(
            "POST",
            f"{owner_repo}/git/refs",
            {"ref": f"refs/heads/{branch}", "sha": sha},
            token=token,
        )
    if st not in (200, 201):
        die(f"aggiornamento ref {branch} fallito (HTTP {st}: {r.get('message')})")

    # 6. Verifica: riconteggio i blob sull'albero remoto.
    st, remote_tree = req(
        "GET", f"{owner_repo}/git/trees/{sha}?recursive=1", token=token
    )
    blobs = [t for t in remote_tree.get("tree", []) if t.get("type") == "blob"]
    if len(blobs) != total_files:
        die(f"verifica fallita: {len(blobs)} file remoti, attesi {total_files}")
    print(f"VERIFICATO: {len(blobs)} file su https://github.com/{target} (branch {branch})")


def me_msg(d):
    return d.get("message", "?")


if __name__ == "__main__":
    main()
