.intel_syntax noprefix
l:
  vpaddd zmm0, zmm1, zmm2
  vpsubd zmm3, zmm4, zmm5
  dec rcx
  jnz l
