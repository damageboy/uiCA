.intel_syntax noprefix
l:
  vmovdqu ymm0, [rsi]
  vpaddd ymm0, ymm0, ymm1
  vmovdqu [rdi], ymm0
  dec rcx
  jnz l
