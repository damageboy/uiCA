.intel_syntax noprefix
l:
  lea rax, [rbx + rcx*2 + 8]
  lea rdx, [rax + rsi*4 + 32]
  add rdx, rax
  dec r8
  jnz l
