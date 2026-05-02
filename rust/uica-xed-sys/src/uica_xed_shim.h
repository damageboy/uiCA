#ifndef UICA_XED_SHIM_H
#define UICA_XED_SHIM_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define UICA_XED_MAX_REGS 32
#define UICA_XED_MAX_MEMS 4
#define UICA_XED_MAX_EXPLICIT_REGS 16
#define UICA_XED_TEXT_CAP 128
#define UICA_XED_IFORM_CAP 96

#define UICA_XED_STATUS_OK 0
#define UICA_XED_STATUS_INVALID 1
#define UICA_XED_STATUS_TRUNCATED 2

#define UICA_XED_ACCESS_NONE 0
#define UICA_XED_ACCESS_READ 1
#define UICA_XED_ACCESS_WRITE 2
#define UICA_XED_ACCESS_READ_WRITE 3
#define UICA_XED_ACCESS_COND_READ 4
#define UICA_XED_ACCESS_COND_WRITE 5
#define UICA_XED_ACCESS_READ_COND_WRITE 6

typedef struct uica_xed_mem_s {
    char base[16];
    char index[16];
    int32_t scale;
    int64_t disp;
    uint8_t access;
    uint8_t is_implicit_stack_operand;
} uica_xed_mem_t;

typedef struct uica_xed_reg_s {
    char name[16];
    uint8_t access;
    uint8_t explicit_operand;
    uint8_t size_bytes;
} uica_xed_reg_t;

typedef struct uica_xed_inst_s {
    uint8_t status;
    uint32_t len;
    char mnemonic[UICA_XED_TEXT_CAP];
    char disasm[UICA_XED_TEXT_CAP];
    char iform[UICA_XED_IFORM_CAP];
    uint8_t reads_flags;
    uint8_t writes_flags;
    int32_t implicit_rsp_change;
    uint8_t has_immediate;
    uint32_t immediate_width_bits;
    int64_t immediate;
    uint8_t mem_count;
    uica_xed_mem_t mems[UICA_XED_MAX_MEMS];
    uint8_t reg_count;
    uica_xed_reg_t regs[UICA_XED_MAX_REGS];
    uint8_t explicit_reg_count;
    char explicit_regs[UICA_XED_MAX_EXPLICIT_REGS][16];
    uint8_t max_op_size_bytes;
    uint8_t uses_high8_reg;
    char agen[32];
} uica_xed_inst_t;

void uica_xed_init(void);
int uica_xed_decode_one(const uint8_t* bytes, uint32_t len, uint64_t ip, uica_xed_inst_t* out);

#ifdef __cplusplus
}
#endif

#endif
