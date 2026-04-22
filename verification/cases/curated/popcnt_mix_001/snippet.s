.intel_syntax noprefix
l:
  popcnt rax, rbx
  add rcx, rax
  xor rdx, rcx
  dec r8
  jnz l
