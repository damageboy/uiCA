.intel_syntax noprefix
l:
  pclmulqdq xmm0, xmm1, 0x10
  pxor xmm2, xmm0
  paddq xmm3, xmm2
  dec rcx
  jnz l
