.intel_syntax noprefix
l:
  mov al, bl
  movzx ecx, al
  add rax, rcx
  dec rdx
  jnz l
