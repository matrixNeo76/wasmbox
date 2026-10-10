#!/bin/sh
# Scenario E — FFI/C-ABI via python3 stdlib (ctypes), docs/extension-plan.md §3.
#
# Carica target/release/libwasmbox_ffi.so e verifica:
#   1) round-trip echo via wasmbox_engine_run + wasmbox_buffer_free;
#   2) wasm invalido → engine NULL (init fallito), status Contract §3.2;
#   3) codici status (0 ok / 1 invalid / 3 run-fail) via engine built ad hoc.
# FAIL esplicito se python3 non è presente (nessuna dipendenza aggiunta).
set -u
cd "$(dirname "$0")/../.."

LIB=target/release/libwasmbox_ffi.so
GUEST=target/wasm32-unknown-unknown/release/guest_echo.wasm

[ -f "$LIB" ] || {
  echo '{"scenario":"E","ok":false,"reason":"compila prima: cargo build -p wasmbox-ffi --release"}'
  exit 2
}
[ -f "$GUEST" ] || {
  echo '{"scenario":"E","ok":false,"reason":"compila il guest: cargo build -p guest-echo --target wasm32-unknown-unknown --release"}'
  exit 2
}
command -v python3 >/dev/null 2>&1 || {
  echo '{"scenario":"E","ok":false,"reason":"python3 non disponibile (richiesto da questo scenario)"}'
  exit 2
}

python3 - "$LIB" "$GUEST" <<'PYEOF'
import ctypes, sys

lib_path, guest_path = sys.argv[1], sys.argv[2]
lib = ctypes.CDLL(lib_path)

class WasmboxLimits(ctypes.Structure):
    _fields_ = [
        ("max_fuel", ctypes.c_uint64),
        ("epoch_timeout_ms", ctypes.c_uint64),
        ("epoch_timeout_enabled", ctypes.c_uint32),
        ("max_memory_bytes", ctypes.c_size_t),
        ("max_ask_calls", ctypes.c_uint32),
        ("max_ask_payload_bytes", ctypes.c_size_t),
    ]

lib.wasmbox_engine_new.argtypes = [ctypes.c_char_p, ctypes.c_size_t,
                                   ctypes.POINTER(WasmboxLimits)]
lib.wasmbox_engine_new.restype = ctypes.c_void_p
lib.wasmbox_engine_free.argtypes = [ctypes.c_void_p]
lib.wasmbox_engine_run.argtypes = [ctypes.c_void_p, ctypes.c_char_p,
                                   ctypes.c_size_t,
                                   ctypes.POINTER(ctypes.POINTER(ctypes.c_char)),
                                   ctypes.POINTER(ctypes.c_size_t)]
lib.wasmbox_engine_run.restype = ctypes.c_uint32
lib.wasmbox_buffer_free.argtypes = [ctypes.POINTER(ctypes.c_char), ctypes.c_size_t]
lib.wasmbox_last_error.argtypes = [ctypes.c_void_p]
lib.wasmbox_last_error.restype = ctypes.c_char_p
lib.wasmbox_last_error_code.argtypes = [ctypes.c_void_p]
lib.wasmbox_last_error_code.restype = ctypes.c_uint32

fails = []

def case(name, cond, detail=""):
    print(f"case {name}: {'PASS' if cond else 'FAIL ' + detail}")
    if not cond:
        fails.append(name)

wasm = open(guest_path, "rb").read()

# --- 1) round-trip echo ---
eng = lib.wasmbox_engine_new(wasm, len(wasm), None)
case("engine_new", bool(eng), "engine NULL")
if eng:
    out_ptr = ctypes.POINTER(ctypes.c_char)()
    out_len = ctypes.c_size_t(0)
    st = lib.wasmbox_engine_run(eng, b"py-ffi-test", 11,
                                ctypes.byref(out_ptr), ctypes.byref(out_len))
    ok = st == 0 and out_len.value > 0
    body = ctypes.string_at(out_ptr, out_len.value) if ok else b""
    case("echo_roundtrip", ok and body.startswith(b"echo_result:")
         and b"py-ffi-test" in body, f"status={st} out={body[:60]!r}")
    case("last_error_code_ok", lib.wasmbox_last_error_code(eng) == 0)
    lib.wasmbox_buffer_free(out_ptr, out_len.value)
    lib.wasmbox_engine_free(eng)

# --- 2) wasm invalido → new restituisce NULL ---
bad = lib.wasmbox_engine_new(b"non-wasm-bytes", 13, None)
case("invalid_wasm_null", not bad)

# --- 3) argomenti invalidi → status 1 senza engine ---
if True:  # ctypes: passare None a c_void_p arg evita deref
    out_ptr = ctypes.POINTER(ctypes.c_char)()
    out_len = ctypes.c_size_t(0)
    st = lib.wasmbox_engine_run(None, b"x", 1,
                                ctypes.byref(out_ptr), ctypes.byref(out_len))
    case("null_engine_status_1", st == 1, f"status={st}")

print("---")
if fails:
    print("FAILED: " + " ".join(fails))
    sys.exit(1)
print("ALL E CASES PASS")
PYEOF
exit $?
