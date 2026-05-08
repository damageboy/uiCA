use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecodedMemAddr {
    pub base: Option<String>,
    pub index: Option<String>,
    pub scale: i32,
    pub disp: i64,
    pub is_implicit_stack_operand: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecodedInstruction {
    pub ip: u64,
    pub len: u32,
    pub mnemonic: String,
    pub disasm: String,
    pub bytes: Vec<u8>,
    pub pos_nominal_opcode: u32,
    pub input_regs: Vec<String>,
    pub output_regs: Vec<String>,
    pub reads_flags: bool,
    pub writes_flags: bool,
    pub has_memory_read: bool,
    pub has_memory_write: bool,
    pub mem_addrs: Vec<DecodedMemAddr>,
    pub implicit_rsp_change: i32,
    pub immediate: Option<i64>,
    pub immediate_width_bits: u32,
    pub has_66_prefix: bool,
    pub iform: String,
    pub iform_signature: String,
    pub max_op_size_bytes: u8,
    pub uses_high8_reg: bool,
    pub explicit_reg_operands: Vec<String>,
    pub agen: Option<String>,
    pub xml_attrs: BTreeMap<String, String>,
}
