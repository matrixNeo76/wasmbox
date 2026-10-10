(module
  (memory (export "memory") 2)
  ;; Bomba OOM: guest_alloc ritorna 0 (out-of-memory) per alloc > 1 MiB.
  ;; La bomba è l'INPUT da 20 MiB (via stdin): il core chiama guest_alloc(20 MiB),
  ;; riceve 0 → SandboxError::GuestOutOfMemory → exit 5, senza trap né crash.
  (func (export "guest_alloc") (param $len i32) (result i32)
    (if (i32.gt_u (local.get $len) (i32.const 1048576))
      (then (return (i32.const 0))))
    (if (i32.eqz (local.get $len)) (then (return (i32.const 1024))))
    (i32.const 2048))
  (func (export "guest_free") (param i32 i32))
  (func (export "guest_run") (param i32 i32) (result i64)
    (i64.const 0))
)
