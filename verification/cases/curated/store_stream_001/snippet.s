.intel_syntax noprefix
l:
  mov [rdi], rax
  mov [rdi+8], rbx
  add rdi, 16
  dec rcx
  jnz l
