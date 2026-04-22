.intel_syntax noprefix
l:
  movdqa xmm0, xmm1
  paddq xmm0, xmm2
  pxor xmm3, xmm0
  dec rcx
  jnz l
