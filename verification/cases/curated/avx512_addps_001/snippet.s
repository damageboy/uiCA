.intel_syntax noprefix
l:
  vaddps zmm0, zmm1, zmm2
  vaddps zmm3, zmm4, zmm5
  dec rcx
  jnz l
