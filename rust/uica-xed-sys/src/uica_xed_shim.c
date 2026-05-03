#include "uica_xed_shim.h"
#include "xed/xed-interface.h"

#include <ctype.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

static void uica_xed_copy_text(char* dst, size_t cap, const char* src) {
   size_t i;

   if (cap == 0) {
      return;
   }
   if (src == 0) {
      dst[0] = 0;
      return;
   }

   for (i = 0; i + 1 < cap && src[i] != 0; ++i) {
      dst[i] = src[i];
   }
   dst[i] = 0;
}

static void uica_xed_lowercase(char* text) {
   unsigned char* p = (unsigned char*)text;
   while (*p != 0) {
      *p = (unsigned char)tolower(*p);
      ++p;
   }
}

static uint8_t uica_xed_access_from_action(xed_operand_action_enum_t action) {
   xed_uint_t reads = xed_operand_action_read(action);
   xed_uint_t writes = xed_operand_action_written(action);
   xed_uint_t cond_reads = xed_operand_action_conditional_read(action);
   xed_uint_t cond_writes = xed_operand_action_conditional_write(action);

   if (reads && writes) {
      if (cond_writes) {
         return UICA_XED_ACCESS_READ_COND_WRITE;
      }
      return UICA_XED_ACCESS_READ_WRITE;
   }
   if (reads) {
      return cond_reads ? UICA_XED_ACCESS_COND_READ : UICA_XED_ACCESS_READ;
   }
   if (writes) {
      return cond_writes ? UICA_XED_ACCESS_COND_WRITE : UICA_XED_ACCESS_WRITE;
   }
   return UICA_XED_ACCESS_NONE;
}

static uint8_t uica_xed_access_from_memory(const xed_decoded_inst_t* xedd, unsigned int mem_idx) {
   xed_bool_t reads = xed_decoded_inst_mem_read(xedd, mem_idx);
   xed_bool_t writes = xed_decoded_inst_mem_written(xedd, mem_idx);

   if (reads && writes) {
      return UICA_XED_ACCESS_READ_WRITE;
   }
   if (reads) {
      return UICA_XED_ACCESS_READ;
   }
   if (writes) {
      return UICA_XED_ACCESS_WRITE;
   }
   return UICA_XED_ACCESS_NONE;
}

static int uica_xed_is_invalid_reg(xed_reg_enum_t reg) {
   return reg == XED_REG_INVALID || reg == XED_REG_LAST;
}

static int uica_xed_is_metadata_reg(xed_reg_enum_t reg) {
   switch (reg) {
      case XED_REG_RFLAGS:
      case XED_REG_EFLAGS:
      case XED_REG_FLAGS:
      case XED_REG_RIP:
      case XED_REG_EIP:
      case XED_REG_IP:
      case XED_REG_STACKPUSH:
      case XED_REG_STACKPOP:
         return 1;
      default:
         return 0;
   }
}

static int uica_xed_is_high8_reg_name(const char* name) {
   return strcmp(name, "AH") == 0 || strcmp(name, "BH") == 0 ||
          strcmp(name, "CH") == 0 || strcmp(name, "DH") == 0;
}

static void uica_xed_append_high8_operand(uica_xed_inst_t* out, xed_operand_enum_t operand) {
   const char* op_name = xed_operand_enum_t2str(operand);
   size_t len;

   if (out->high8[0] != 0) {
      strncat(out->high8, ",", sizeof(out->high8) - strlen(out->high8) - 1);
   }
   len = strlen(out->high8);
   if (len + 1 < sizeof(out->high8)) {
      strncat(out->high8, op_name, sizeof(out->high8) - len - 1);
   }
}

static void uica_xed_copy_reg_name(char* dst, size_t cap, xed_reg_enum_t reg) {
   if (uica_xed_is_invalid_reg(reg)) {
      uica_xed_copy_text(dst, cap, "");
      return;
   }
   uica_xed_copy_text(dst, cap, xed_reg_enum_t2str(reg));
}

static void uica_xed_append_reg(
   uica_xed_inst_t* out,
   xed_reg_enum_t reg,
   uint8_t access,
   uint8_t explicit_operand,
   uint8_t size_bytes
) {
   uica_xed_reg_t* dst;
   char name[16];

   if (access == UICA_XED_ACCESS_NONE || uica_xed_is_invalid_reg(reg) ||
       uica_xed_is_metadata_reg(reg)) {
      return;
   }
   if (out->reg_count >= UICA_XED_MAX_REGS) {
      return;
   }

   uica_xed_copy_reg_name(name, sizeof(name), reg);
   if (name[0] == 0) {
      return;
   }

   dst = &out->regs[out->reg_count++];
   uica_xed_copy_text(dst->name, sizeof(dst->name), name);
   dst->access = access;
   dst->explicit_operand = explicit_operand;
   dst->size_bytes = size_bytes;

   if (explicit_operand) {
      if (out->explicit_reg_count < UICA_XED_MAX_EXPLICIT_REGS) {
         uica_xed_copy_text(
            out->explicit_regs[out->explicit_reg_count++],
            sizeof(out->explicit_regs[0]),
            name
         );
      }
      if (size_bytes > out->max_op_size_bytes) {
         out->max_op_size_bytes = size_bytes;
      }
      if (uica_xed_is_high8_reg_name(name)) {
         out->uses_high8_reg = 1;
      }
   }
}

static int uica_xed_is_stack_push_mem_operand(const xed_decoded_inst_t* xedd, unsigned int mem_idx) {
   if (mem_idx == 0) {
      return xed_decoded_inst_get_attribute(xedd, XED_ATTRIBUTE_STACKPUSH0) ? 1 : 0;
   }
   if (mem_idx == 1) {
      return xed_decoded_inst_get_attribute(xedd, XED_ATTRIBUTE_STACKPUSH1) ? 1 : 0;
   }
   return 0;
}

static int uica_xed_is_stack_pop_mem_operand(const xed_decoded_inst_t* xedd, unsigned int mem_idx) {
   if (mem_idx == 0) {
      return xed_decoded_inst_get_attribute(xedd, XED_ATTRIBUTE_STACKPOP0) ? 1 : 0;
   }
   if (mem_idx == 1) {
      return xed_decoded_inst_get_attribute(xedd, XED_ATTRIBUTE_STACKPOP1) ? 1 : 0;
   }
   return 0;
}

static int uica_xed_is_stack_mem_operand(const xed_decoded_inst_t* xedd, unsigned int mem_idx) {
   return uica_xed_is_stack_push_mem_operand(xedd, mem_idx) ||
          uica_xed_is_stack_pop_mem_operand(xedd, mem_idx);
}

static int32_t uica_xed_fallback_stack_change(xed_iclass_enum_t iclass) {
   switch (iclass) {
      case XED_ICLASS_PUSH:
      case XED_ICLASS_PUSHA:
      case XED_ICLASS_PUSHAD:
      case XED_ICLASS_PUSHF:
      case XED_ICLASS_PUSHFD:
      case XED_ICLASS_PUSHFQ:
      case XED_ICLASS_CALL_NEAR:
      case XED_ICLASS_CALL_FAR:
      case XED_ICLASS_ENTER:
         return -8;
      case XED_ICLASS_POP:
      case XED_ICLASS_POPA:
      case XED_ICLASS_POPAD:
      case XED_ICLASS_POPF:
      case XED_ICLASS_POPFD:
      case XED_ICLASS_POPFQ:
      case XED_ICLASS_RET_NEAR:
      case XED_ICLASS_RET_FAR:
         return 8;
      default:
         return 0;
   }
}

static int32_t uica_xed_implicit_rsp_change(const xed_decoded_inst_t* xedd) {
   unsigned int count = xed_decoded_inst_number_of_memory_operands(xedd);
   unsigned int i;

   for (i = 0; i < count; ++i) {
      if (uica_xed_is_stack_push_mem_operand(xedd, i)) {
         unsigned int len = xed_decoded_inst_get_memory_operand_length(xedd, i);
         if (len != 0) {
            return -(int32_t)len;
         }
      }
      if (uica_xed_is_stack_pop_mem_operand(xedd, i)) {
         unsigned int len = xed_decoded_inst_get_memory_operand_length(xedd, i);
         if (len != 0) {
            return (int32_t)len;
         }
      }
   }

   return uica_xed_fallback_stack_change(xed_decoded_inst_get_iclass(xedd));
}

static void uica_xed_copy_flags(const xed_decoded_inst_t* xedd, uica_xed_inst_t* out) {
   const xed_simple_flag_t* flags = xed_decoded_inst_get_rflags_info(xedd);
   if (flags == 0) {
      return;
   }
   out->reads_flags = xed_simple_flag_reads_flags(flags) ? 1 : 0;
   out->writes_flags = xed_simple_flag_writes_flags(flags) ? 1 : 0;
}

static void uica_xed_copy_immediate(const xed_decoded_inst_t* xedd, uica_xed_inst_t* out) {
   unsigned int width = xed_decoded_inst_get_immediate_width_bits(xedd);
   if (width == 0) {
      return;
   }
   out->has_immediate = 1;
   out->immediate_width_bits = width;
   out->immediate = xed_decoded_inst_get_signed_immediate(xedd);
   if (out->immediate == 0) {
      out->immzero = 1;
   }
}

static void uica_xed_copy_match_attrs(const xed_decoded_inst_t* xedd, uica_xed_inst_t* out) {
   out->bcast = xed3_operand_get_bcast(xedd);
   out->eosz = xed3_operand_get_eosz(xedd);
   out->mask = xed3_operand_get_mask(xedd) ? 1 : 0;
   out->rep = xed3_operand_get_rep(xedd);
   out->rm = xed3_operand_get_rm(xedd);
   out->sae = xed3_operand_get_sae(xedd);
   out->zeroing = xed3_operand_get_zeroing(xedd);
}

static void uica_xed_copy_registers(const xed_decoded_inst_t* xedd, uica_xed_inst_t* out) {
   const xed_inst_t* xi = xed_decoded_inst_inst(xedd);
   unsigned int noperands = xed_inst_noperands(xi);
   unsigned int i;

   for (i = 0; i < noperands; ++i) {
      const xed_operand_t* operand = xed_inst_operand(xi, i);
      xed_operand_enum_t name = xed_operand_name(operand);
      xed_operand_visibility_enum_t visibility;
      xed_operand_action_enum_t action;
      xed_reg_enum_t reg;
      unsigned int bits;
      uint8_t size_bytes;

      if (!xed_operand_is_register(name)) {
         continue;
      }

      reg = xed_decoded_inst_get_reg(xedd, name);
      if (uica_xed_is_invalid_reg(reg)) {
         continue;
      }

      action = xed_decoded_inst_operand_action(xedd, i);
      bits = xed_decoded_inst_operand_length_bits(xedd, i);
      size_bytes = (uint8_t)((bits + 7) / 8);
      visibility = xed_operand_operand_visibility(operand);

      uica_xed_append_reg(
         out,
         reg,
         uica_xed_access_from_action(action),
         visibility == XED_OPVIS_EXPLICIT ? 1 : 0,
         size_bytes
      );
      if (visibility == XED_OPVIS_EXPLICIT && uica_xed_is_high8_reg_name(xed_reg_enum_t2str(reg))) {
         uica_xed_append_high8_operand(out, name);
      }
   }
}

static void uica_xed_copy_memories(const xed_decoded_inst_t* xedd, uica_xed_inst_t* out) {
   unsigned int count = xed_decoded_inst_number_of_memory_operands(xedd);
   unsigned int i;

   if (count > UICA_XED_MAX_MEMS) {
      count = UICA_XED_MAX_MEMS;
   }

   for (i = 0; i < count; ++i) {
      uica_xed_mem_t* mem = &out->mems[out->mem_count++];
      xed_reg_enum_t base = xed_decoded_inst_get_base_reg(xedd, i);
      xed_reg_enum_t index = xed_decoded_inst_get_index_reg(xedd, i);

      uica_xed_copy_reg_name(mem->base, sizeof(mem->base), base);
      uica_xed_copy_reg_name(mem->index, sizeof(mem->index), index);
      mem->scale = (int32_t)xed_decoded_inst_get_scale(xedd, i);
      mem->disp = xed_decoded_inst_get_memory_displacement(xedd, i);
      mem->access = uica_xed_access_from_memory(xedd, i);
      mem->is_implicit_stack_operand = uica_xed_is_stack_mem_operand(xedd, i) ? 1 : 0;
   }
}

static void uica_xed_derive_agen(const xed_decoded_inst_t* xedd, uica_xed_inst_t* out) {
   xed_reg_enum_t base;
   xed_reg_enum_t index;
   unsigned int scale;
   unsigned int disp_width;
   int has_part = 0;

   if (xed_decoded_inst_get_iclass(xedd) != XED_ICLASS_LEA) {
      return;
   }

   base = xed_decoded_inst_get_base_reg(xedd, 0);
   index = xed_decoded_inst_get_index_reg(xedd, 0);
   scale = xed_decoded_inst_get_scale(xedd, 0);
   disp_width = xed_decoded_inst_get_memory_displacement_width(xedd, 0);

   if (base == XED_REG_RIP || base == XED_REG_EIP) {
      uica_xed_copy_text(out->agen, sizeof(out->agen), "R");
      has_part = 1;
   } else if (!uica_xed_is_invalid_reg(base)) {
      uica_xed_copy_text(out->agen, sizeof(out->agen), "B");
      has_part = 1;
   }

   if (!uica_xed_is_invalid_reg(index)) {
      if (has_part) {
         strncat(out->agen, "_", sizeof(out->agen) - strlen(out->agen) - 1);
      }
      strncat(out->agen, scale == 1 ? "I" : "IS", sizeof(out->agen) - strlen(out->agen) - 1);
      has_part = 1;
   }

   if (disp_width != 0) {
      if (has_part) {
         strncat(out->agen, "_", sizeof(out->agen) - strlen(out->agen) - 1);
      }
      strncat(out->agen, disp_width == 1 ? "D8" : "D32", sizeof(out->agen) - strlen(out->agen) - 1);
      has_part = 1;
   }

   if (!has_part) {
      uica_xed_copy_text(out->agen, sizeof(out->agen), "D32");
   }
}

void uica_xed_init(void) {
   static atomic_int init_state = ATOMIC_VAR_INIT(0);
   int expected = 0;

   if (atomic_compare_exchange_strong_explicit(
         &init_state,
         &expected,
         1,
         memory_order_acq_rel,
         memory_order_acquire
      )) {
      xed_tables_init();
      atomic_store_explicit(&init_state, 2, memory_order_release);
      return;
   }

   while (atomic_load_explicit(&init_state, memory_order_acquire) != 2) {
   }
}

int uica_xed_decode_one(const uint8_t* bytes, uint32_t len, uint64_t ip, uica_xed_inst_t* out) {
   xed_state_t dstate;
   xed_decoded_inst_t xedd;
   xed_error_enum_t err;
   unsigned int max_decode;
   const char* mnemonic;
   const char* iform;

   if (out == 0) {
      return -1;
   }

   memset(out, 0, sizeof(*out));
   out->status = UICA_XED_STATUS_INVALID;

   if (bytes == 0 || len == 0) {
      return 0;
   }

   uica_xed_init();

   max_decode = len < 15 ? len : 15;
   xed_state_init2(&dstate, XED_MACHINE_MODE_LONG_64, XED_ADDRESS_WIDTH_64b);
   xed_decoded_inst_zero_set_mode(&xedd, &dstate);

   err = xed_decode(&xedd, bytes, max_decode);
   if (err != XED_ERROR_NONE) {
      if (err == XED_ERROR_BUFFER_TOO_SHORT) {
         out->status = UICA_XED_STATUS_TRUNCATED;
      }
      return 0;
   }

   out->status = UICA_XED_STATUS_OK;
   out->len = xed_decoded_inst_get_length(&xedd);
   out->pos_nominal_opcode = xed3_operand_get_pos_nominal_opcode(&xedd);
   out->implicit_rsp_change = uica_xed_implicit_rsp_change(&xedd);

   mnemonic = xed_iclass_enum_t2str(xed_decoded_inst_get_iclass(&xedd));
   uica_xed_copy_text(out->mnemonic, UICA_XED_TEXT_CAP, mnemonic);
   uica_xed_lowercase(out->mnemonic);

   (void)xed_format_context(
      XED_SYNTAX_INTEL,
      &xedd,
      out->disasm,
      UICA_XED_TEXT_CAP,
      ip,
      0,
      0
   );
   uica_xed_lowercase(out->disasm);

   iform = xed_iform_enum_t2str(xed_decoded_inst_get_iform_enum(&xedd));
   uica_xed_copy_text(out->iform, UICA_XED_IFORM_CAP, iform);

   uica_xed_copy_flags(&xedd, out);
   uica_xed_copy_match_attrs(&xedd, out);
   uica_xed_copy_immediate(&xedd, out);
   uica_xed_copy_registers(&xedd, out);
   uica_xed_copy_memories(&xedd, out);
   uica_xed_derive_agen(&xedd, out);

   return 0;
}
