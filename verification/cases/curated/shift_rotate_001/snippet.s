.intel_syntax noprefix
l:
  shl rax, cl
  ror rbx, 1
  sar rdx, 3
  dec r8
  jnz l
