.intel_syntax noprefix
l:
  aesenc xmm0, xmm1
  aesenclast xmm0, xmm2
  pxor xmm3, xmm0
  dec rcx
  jnz l
