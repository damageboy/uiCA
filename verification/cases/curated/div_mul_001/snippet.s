.intel_syntax noprefix
l:
  xor edx, edx
  mov eax, esi
  div ecx
  imul r8, r9
  dec r10
  jnz l
