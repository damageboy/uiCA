.intel_syntax noprefix
l:
  cmp rax, rbx
  setne al
  movzx ecx, al
  add rdx, rcx
  dec r8
  jnz l
