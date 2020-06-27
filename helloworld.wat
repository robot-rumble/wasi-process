(module
  (import "wasi_unstable" "fd_write" (func $fd_write (param i32 i32 i32 i32) (result i32)))

  (memory 1)
  (export "memory" (memory 0))

  (data (i32.const 12) "Hello, World!\n")

  (func $main (export "_start")
        (i32.store (i32.const 0) (i32.const 12))
        (i32.store (i32.const 4) (i32.const 14))
        (call $fd_write
              (i32.const 1)
              (i32.const 0)
              (i32.const 1)
              (i32.const 14)
         )
    drop
  )
)
