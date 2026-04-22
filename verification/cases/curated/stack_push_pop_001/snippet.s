.intel_syntax noprefix
l:
  push rax
  pop rbx
  add rbx, rcx
  dec rdx
  jnz l
