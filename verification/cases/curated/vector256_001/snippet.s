.intel_syntax noprefix
l:
  vaddps ymm0, ymm1, ymm2
  vmulps ymm3, ymm4, ymm5
  vpxor ymm6, ymm6, ymm0
  dec rcx
  jnz l
