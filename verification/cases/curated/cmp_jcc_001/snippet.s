.intel_syntax noprefix
l:
  cmp rax, rbx
  jne .Lskip
  add rcx, rdx
.Lskip:
  dec r8
  jnz l
