.intel_syntax noprefix
l:
  cmp rax, rbx
  jg .Lhot
  sub rcx, rdx
.Lhot:
  add r8, r9
  dec r10
  jnz l
