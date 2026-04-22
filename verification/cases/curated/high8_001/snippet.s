.intel_syntax noprefix
l:
  mov ah, bl
  add eax, ecx
  movzx edx, ah
  dec r8
  jnz l
