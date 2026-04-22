.intel_syntax noprefix
l:
  lzcnt rax, rbx
  tzcnt rcx, rdx
  add r8, rax
  dec r9
  jnz l
