.intel_syntax noprefix
l:
  mov rax, rbx
  mov rcx, rax
  mov rdx, rcx
  add rdx, rsi
  dec r8
  jnz l
