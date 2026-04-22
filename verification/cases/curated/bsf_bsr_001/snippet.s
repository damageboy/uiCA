.intel_syntax noprefix
l:
  bsf rax, rbx
  bsr rcx, rdx
  xor r8, rax
  dec r9
  jnz l
