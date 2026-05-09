#![allow(non_camel_case_types)]

use std::os::raw::{c_char, c_int};

pub const UICA_XED_MAX_REGS: usize = 32;
pub const UICA_XED_MAX_MEMS: usize = 4;
pub const UICA_XED_MAX_EXPLICIT_REGS: usize = 16;
pub const UICA_XED_TEXT_CAP: usize = 128;
pub const UICA_XED_IFORM_CAP: usize = 96;
pub const UICA_XED_HIGH8_CAP: usize = 64;

pub const UICA_XED_STATUS_OK: u8 = 0;
pub const UICA_XED_STATUS_INVALID: u8 = 1;
pub const UICA_XED_STATUS_TRUNCATED: u8 = 2;

pub const UICA_XED_ACCESS_NONE: u8 = 0;
pub const UICA_XED_ACCESS_READ: u8 = 1;
pub const UICA_XED_ACCESS_WRITE: u8 = 2;
pub const UICA_XED_ACCESS_READ_WRITE: u8 = 3;
pub const UICA_XED_ACCESS_COND_READ: u8 = 4;
pub const UICA_XED_ACCESS_COND_WRITE: u8 = 5;
pub const UICA_XED_ACCESS_READ_COND_WRITE: u8 = 6;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct uica_xed_mem_t {
    pub base: [c_char; 16],
    pub index: [c_char; 16],
    pub scale: i32,
    pub disp: i64,
    pub access: u8,
    pub is_implicit_stack_operand: u8,
}

impl Default for uica_xed_mem_t {
    fn default() -> Self {
        Self {
            base: [0; 16],
            index: [0; 16],
            scale: 0,
            disp: 0,
            access: UICA_XED_ACCESS_NONE,
            is_implicit_stack_operand: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct uica_xed_reg_t {
    pub name: [c_char; 16],
    pub access: u8,
    pub explicit_operand: u8,
    pub size_bytes: u8,
}

impl Default for uica_xed_reg_t {
    fn default() -> Self {
        Self {
            name: [0; 16],
            access: UICA_XED_ACCESS_NONE,
            explicit_operand: 0,
            size_bytes: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct uica_xed_inst_t {
    pub status: u8,
    pub len: u32,
    pub pos_nominal_opcode: u32,
    pub mnemonic: [c_char; UICA_XED_TEXT_CAP],
    pub disasm: [c_char; UICA_XED_TEXT_CAP],
    pub iform: [c_char; UICA_XED_IFORM_CAP],
    pub reads_flags: u8,
    pub writes_flags: u8,
    pub implicit_rsp_change: i32,
    pub has_immediate: u8,
    pub immediate_width_bits: u32,
    pub immediate: i64,
    pub mem_count: u8,
    pub mems: [uica_xed_mem_t; UICA_XED_MAX_MEMS],
    pub reg_count: u8,
    pub regs: [uica_xed_reg_t; UICA_XED_MAX_REGS],
    pub explicit_reg_count: u8,
    pub explicit_regs: [[c_char; 16]; UICA_XED_MAX_EXPLICIT_REGS],
    pub max_op_size_bytes: u8,
    pub uses_high8_reg: u8,
    pub agen: [c_char; 32],
    pub high8: [c_char; UICA_XED_HIGH8_CAP],
    pub bcast: u32,
    pub eosz: u32,
    pub mask: u32,
    pub rep: u32,
    pub rm: u32,
    pub sae: u32,
    pub zeroing: u32,
    pub immzero: u8,
}

impl Default for uica_xed_inst_t {
    fn default() -> Self {
        Self {
            status: 0,
            len: 0,
            pos_nominal_opcode: 0,
            mnemonic: [0; UICA_XED_TEXT_CAP],
            disasm: [0; UICA_XED_TEXT_CAP],
            iform: [0; UICA_XED_IFORM_CAP],
            reads_flags: 0,
            writes_flags: 0,
            implicit_rsp_change: 0,
            has_immediate: 0,
            immediate_width_bits: 0,
            immediate: 0,
            mem_count: 0,
            mems: [uica_xed_mem_t::default(); UICA_XED_MAX_MEMS],
            reg_count: 0,
            regs: [uica_xed_reg_t::default(); UICA_XED_MAX_REGS],
            explicit_reg_count: 0,
            explicit_regs: [[0; 16]; UICA_XED_MAX_EXPLICIT_REGS],
            max_op_size_bytes: 0,
            uses_high8_reg: 0,
            agen: [0; 32],
            high8: [0; UICA_XED_HIGH8_CAP],
            bcast: 0,
            eosz: 0,
            mask: 0,
            rep: 0,
            rm: 0,
            sae: 0,
            zeroing: 0,
            immzero: 0,
        }
    }
}

#[cfg(any(not(target_arch = "wasm32"), target_os = "emscripten"))]
extern "C" {
    pub fn uica_xed_init();
    pub fn uica_xed_decode_one(
        bytes: *const u8,
        len: u32,
        ip: u64,
        out: *mut uica_xed_inst_t,
    ) -> c_int;
}

#[cfg(all(target_arch = "wasm32", not(target_os = "emscripten")))]
pub unsafe fn uica_xed_init() {}

#[cfg(all(target_arch = "wasm32", not(target_os = "emscripten")))]
pub unsafe fn uica_xed_decode_one(
    _bytes: *const u8,
    _len: u32,
    _ip: u64,
    out: *mut uica_xed_inst_t,
) -> c_int {
    if let Some(out) = unsafe { out.as_mut() } {
        out.status = UICA_XED_STATUS_INVALID;
    }
    0
}
