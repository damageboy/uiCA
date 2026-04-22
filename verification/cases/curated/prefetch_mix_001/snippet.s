.intel_syntax noprefix
l:
  prefetcht0 [rsi+64]
  add rax, [rsi]
  add rsi, 64
  dec rcx
  jnz l
