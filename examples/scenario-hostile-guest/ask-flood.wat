(module
  (import "env" "ask" (func $ask (param i32 i32) (result i64)))
  (memory (export "memory") 1)
  (func (export "guest_alloc") (param i32) (result i32)
    (if (i32.eqz (local.get 0)) (then (return (i32.const 1024))))
    (i32.const 1024))
  (func (export "guest_free") (param i32 i32))
  ;; 100.000 chiamate ask di fila: il tetto della sandbox (1024 default)
  ;; deve scattare con AskLimitExceeded ben prima della fine del ciclo.
  (func (export "guest_run") (param i32 i32) (result i64)
    (local $i i32)
    ;; buffer richiesta (4 byte "ping") già scritti dall'host in input;
    ;; riusiamo semplicemente l'input come payload richiesto.
    (loop $flood
      (drop (call $ask (local.get 0) (local.get 1)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br_if $flood (i32.lt_u (local.get $i) (i32.const 100000)))
    )
    (i64.const 0))
)
