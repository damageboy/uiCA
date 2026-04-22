.intel_syntax noprefix
l:
  vfmadd231ps ymm0, ymm1, ymm2
  vfmadd231ps ymm3, ymm4, ymm5
  dec rcx
  jnz l
