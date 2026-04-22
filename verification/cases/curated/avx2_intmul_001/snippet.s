.intel_syntax noprefix
l:
  vpmulld ymm0, ymm1, ymm2
  vpaddd ymm3, ymm0, ymm4
  vpsubd ymm5, ymm3, ymm6
  dec rcx
  jnz l
