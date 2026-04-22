.intel_syntax noprefix
l:
  mov rax, [rax]
  add rax, rbx
  dec rcx
  jnz l
