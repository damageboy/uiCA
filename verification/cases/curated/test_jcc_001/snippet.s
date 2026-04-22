.intel_syntax noprefix
l:
  test rax, rax
  jz .Lzero
  add rbx, rcx
.Lzero:
  dec rdx
  jnz l
