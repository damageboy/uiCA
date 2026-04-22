.intel_syntax noprefix
l:
  vcmpps ymm0, ymm1, ymm2, 0
  vblendvps ymm3, ymm4, ymm5, ymm0
  dec rcx
  jnz l
