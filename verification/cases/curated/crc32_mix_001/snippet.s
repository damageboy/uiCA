.intel_syntax noprefix
l:
  crc32 eax, ebx
  add ecx, eax
  xor edx, ecx
  dec r8
  jnz l
