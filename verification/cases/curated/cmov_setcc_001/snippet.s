.intel_syntax noprefix
l:
  cmp rax, rbx
  cmovg rcx, rdx
  sete al
  add r8, r9
  dec r10
  jnz l
