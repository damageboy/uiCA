.intel_syntax noprefix
l:
  mov rax, [rsi]
  add rax, rbx
  mov [rdi], rax
  dec rcx
  jnz l
