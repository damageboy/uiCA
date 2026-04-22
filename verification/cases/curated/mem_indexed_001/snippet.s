.intel_syntax noprefix
l:
  mov rax, [rsi + rdx*4 + 16]
  add qword ptr [rdi + rcx*8], rax
  dec r8
  jnz l
