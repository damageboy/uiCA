.intel_syntax noprefix
l:
  kmovw k1, eax
  kandw k2, k1, k1
  korw k3, k2, k1
  dec rcx
  jnz l
