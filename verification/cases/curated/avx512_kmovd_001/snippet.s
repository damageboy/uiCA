.intel_syntax noprefix
l:
  kmovd k1, eax
  kaddd k2, k1, k1
  kord k3, k2, k1
  dec rcx
  jnz l
