.intel_syntax noprefix
l:
  vmovdqu64 zmm0, [rsi]
  vpaddq zmm0, zmm0, zmm1
  vmovdqu64 [rdi], zmm0
  dec rcx
  jnz l
