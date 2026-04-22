.intel_syntax noprefix
l:
  vaddps ymm0, ymm1, ymm2
  vaddps ymm3, ymm4, ymm5
  dec rcx
  jnz l
