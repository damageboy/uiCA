.intel_syntax noprefix
l:
  add rax, [rsi]
  lfence
  add rbx, [rdi]
  sfence
  dec rcx
  jnz l
