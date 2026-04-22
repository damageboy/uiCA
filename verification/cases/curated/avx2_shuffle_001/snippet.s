.intel_syntax noprefix
l:
  vpshufd ymm0, ymm1, 0x1B
  vperm2f128 ymm2, ymm3, ymm4, 0x21
  vpblendd ymm5, ymm5, ymm0, 0xAA
  dec rcx
  jnz l
