#[cfg(not(target_arch = "wasm32"))]
use std::os::raw::c_char;

#[cfg(not(target_arch = "wasm32"))]
use anyhow::bail;
use anyhow::Result;
#[cfg(not(target_arch = "wasm32"))]
use uica_xed_sys::{
    uica_xed_decode_one, uica_xed_inst_t, UICA_XED_ACCESS_COND_READ, UICA_XED_ACCESS_COND_WRITE,
    UICA_XED_ACCESS_READ, UICA_XED_ACCESS_READ_COND_WRITE, UICA_XED_ACCESS_READ_WRITE,
    UICA_XED_ACCESS_WRITE, UICA_XED_STATUS_INVALID, UICA_XED_STATUS_OK, UICA_XED_STATUS_TRUNCATED,
};

pub use uica_xed_sys as sys;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedMemAddr {
    pub base: Option<String>,
    pub index: Option<String>,
    pub scale: i32,
    pub disp: i64,
    pub is_implicit_stack_operand: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
    pub iform_signature: String,
    pub max_op_size_bytes: u8,
    pub uses_high8_reg: bool,
    pub explicit_reg_operands: Vec<String>,
    pub agen: Option<String>,
}

#[cfg(target_arch = "wasm32")]
pub fn decode_raw(_bytes: &[u8]) -> Result<Vec<DecodedInstruction>> {
    anyhow::bail!("Intel XED decoder is not available for wasm32 target")
}

#[cfg(not(target_arch = "wasm32"))]
pub fn decode_raw(bytes: &[u8]) -> Result<Vec<DecodedInstruction>> {
    let mut offset = 0usize;
    let mut instructions = Vec::new();

    while offset < bytes.len() {
        let mut raw = uica_xed_inst_t::default();
        let remaining = &bytes[offset..];
        let rc = unsafe {
            uica_xed_decode_one(
                remaining.as_ptr(),
                remaining.len() as u32,
                offset as u64,
                &mut raw,
            )
        };
        if rc != 0 {
            bail!("decoder error {rc} at byte offset {offset}");
        }

        match raw.status {
            UICA_XED_STATUS_OK => {}
            UICA_XED_STATUS_TRUNCATED => {
                bail!("truncated instruction stream at byte offset {offset}")
            }
            UICA_XED_STATUS_INVALID => bail!("invalid instruction at byte offset {offset}"),
            other => bail!("decoder error {other} at byte offset {offset}"),
        }

        let raw_mnemonic = cbuf_to_string(&raw.mnemonic);
        let len = raw.len as usize;
        if len == 0 || len > remaining.len() {
            bail!("truncated instruction stream at byte offset {offset}");
        }

        let instr_bytes = remaining[..len].to_vec();
        let has_66_prefix = instr_bytes
            .iter()
            .take(nominal_opcode_offset(&instr_bytes) as usize)
            .any(|byte| *byte == 0x66);
        let mnemonic = normalize_mnemonic(&raw_mnemonic);
        let disasm = normalize_disasm(&cbuf_to_string(&raw.disasm));
        let iform = cbuf_to_string(&raw.iform);
        let agen = nonempty(cbuf_to_string(&raw.agen));

        let (input_regs, output_regs) = decoded_regs(&raw, &disasm);
        let (has_memory_read, has_memory_write, mem_addrs) = decoded_mem_addrs(&raw);
        let explicit_reg_operands = decoded_explicit_regs(&raw, &disasm);

        instructions.push(DecodedInstruction {
            ip: offset as u64,
            len: raw.len,
            mnemonic,
            disasm,
            bytes: instr_bytes.clone(),
            pos_nominal_opcode: nominal_opcode_offset(&instr_bytes),
            input_regs,
            output_regs,
            reads_flags: raw.reads_flags != 0,
            writes_flags: raw.writes_flags != 0,
            has_memory_read,
            has_memory_write,
            mem_addrs,
            implicit_rsp_change: raw.implicit_rsp_change,
            immediate: if raw.has_immediate != 0 {
                Some(raw.immediate)
            } else {
                None
            },
            immediate_width_bits: raw.immediate_width_bits,
            has_66_prefix,
            iform_signature: iform_to_signature(&iform),
            max_op_size_bytes: raw.max_op_size_bytes,
            uses_high8_reg: raw.uses_high8_reg != 0,
            explicit_reg_operands,
            agen,
        });
        offset += len;
    }

    Ok(instructions)
}

#[cfg(not(target_arch = "wasm32"))]
fn decoded_regs(raw: &uica_xed_inst_t, disasm: &str) -> (Vec<String>, Vec<String>) {
    let mut input_regs = Vec::new();
    let mut output_regs = Vec::new();

    for reg in raw.regs.iter().take(raw.reg_count as usize) {
        let name = cbuf_to_string(&reg.name);
        if name.is_empty() || is_default_k0_mask(&name, disasm) {
            continue;
        }
        if access_reads_register(reg.access) {
            push_unique(&mut input_regs, name.clone());
        }
        if access_writes_register(reg.access) {
            push_unique(&mut output_regs, name);
        }
    }

    for mem in raw.mems.iter().take(raw.mem_count as usize) {
        let base = cbuf_to_string(&mem.base);
        if !base.is_empty() && mem.is_implicit_stack_operand == 0 && !is_metadata_reg_name(&base) {
            push_unique(&mut input_regs, base);
        }
        let index = cbuf_to_string(&mem.index);
        if !index.is_empty() && !is_metadata_reg_name(&index) {
            push_unique(&mut input_regs, index);
        }
    }

    (input_regs, output_regs)
}

#[cfg(not(target_arch = "wasm32"))]
fn decoded_mem_addrs(raw: &uica_xed_inst_t) -> (bool, bool, Vec<DecodedMemAddr>) {
    let mut has_memory_read = false;
    let mut has_memory_write = false;
    let mut mem_addrs = Vec::new();

    for mem in raw.mems.iter().take(raw.mem_count as usize) {
        if access_reads_memory(mem.access) {
            has_memory_read = true;
        }
        if access_writes_memory(mem.access) {
            has_memory_write = true;
        }
        let is_implicit_stack_operand = mem.is_implicit_stack_operand != 0;
        mem_addrs.push(DecodedMemAddr {
            base: empty_to_none(cbuf_to_string(&mem.base)),
            index: empty_to_none(cbuf_to_string(&mem.index)),
            scale: mem.scale,
            disp: if is_implicit_stack_operand {
                0
            } else {
                mem.disp
            },
            is_implicit_stack_operand,
        });
    }

    (has_memory_read, has_memory_write, mem_addrs)
}

#[cfg(not(target_arch = "wasm32"))]
fn decoded_explicit_regs(raw: &uica_xed_inst_t, disasm: &str) -> Vec<String> {
    let mut regs = Vec::new();
    for reg in raw
        .explicit_regs
        .iter()
        .take(raw.explicit_reg_count as usize)
    {
        let name = cbuf_to_string(reg);
        if !name.is_empty() && !is_default_k0_mask(&name, disasm) {
            regs.push(name);
        }
    }
    regs
}

#[cfg(not(target_arch = "wasm32"))]
fn is_default_k0_mask(name: &str, disasm: &str) -> bool {
    // Python parity: `instructions.py` ignores XED's implicit K0 mask unless
    // the assembly text explicitly contains `k0`.
    name == "K0" && !disasm.to_ascii_lowercase().contains("k0")
}

#[cfg(not(target_arch = "wasm32"))]
fn is_metadata_reg_name(name: &str) -> bool {
    matches!(
        name,
        "RIP" | "EIP" | "IP" | "RFLAGS" | "EFLAGS" | "FLAGS" | "STACKPUSH" | "STACKPOP"
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn access_reads_register(access: u8) -> bool {
    matches!(
        access,
        UICA_XED_ACCESS_READ
            | UICA_XED_ACCESS_READ_WRITE
            | UICA_XED_ACCESS_COND_READ
            | UICA_XED_ACCESS_COND_WRITE
            | UICA_XED_ACCESS_READ_COND_WRITE
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn access_writes_register(access: u8) -> bool {
    matches!(
        access,
        UICA_XED_ACCESS_WRITE
            | UICA_XED_ACCESS_READ_WRITE
            | UICA_XED_ACCESS_COND_WRITE
            | UICA_XED_ACCESS_READ_COND_WRITE
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn access_reads_memory(access: u8) -> bool {
    matches!(
        access,
        UICA_XED_ACCESS_READ
            | UICA_XED_ACCESS_READ_WRITE
            | UICA_XED_ACCESS_COND_READ
            | UICA_XED_ACCESS_READ_COND_WRITE
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn access_writes_memory(access: u8) -> bool {
    matches!(
        access,
        UICA_XED_ACCESS_WRITE
            | UICA_XED_ACCESS_READ_WRITE
            | UICA_XED_ACCESS_COND_WRITE
            | UICA_XED_ACCESS_READ_COND_WRITE
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn push_unique(regs: &mut Vec<String>, reg: String) {
    if !regs.iter().any(|existing| existing == &reg) {
        regs.push(reg);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn empty_to_none(value: String) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn cbuf_to_string(buf: &[c_char]) -> String {
    let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
    let bytes: Vec<u8> = buf[..end].iter().map(|b| b.to_ne_bytes()[0]).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(not(target_arch = "wasm32"))]
fn normalize_mnemonic(mnemonic: &str) -> String {
    match mnemonic {
        "call_far" | "call_near" => "call".to_string(),
        "jmp_far" | "jmp_near" => "jmp".to_string(),
        "jnz" => "jne".to_string(),
        "ret_far" | "ret_near" => "ret".to_string(),
        _ => mnemonic.to_string(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn normalize_disasm(disasm: &str) -> String {
    let normalized = disasm
        .replace(',', ", ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    normalized
        .strip_prefix("jnz ")
        .map(|rest| format!("jne {rest}"))
        .unwrap_or(normalized)
}

#[cfg(not(target_arch = "wasm32"))]
fn nonempty(value: String) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn iform_to_signature(iform: &str) -> String {
    let mut parts: Vec<&str> = iform.split('_').skip(1).collect();
    if parts
        .last()
        .is_some_and(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        parts.pop();
    }
    parts
        .into_iter()
        .map(|part| if part == "AGEN" { "MEM" } else { part })
        .collect::<Vec<_>>()
        .join("_")
}

#[cfg(not(target_arch = "wasm32"))]
fn nominal_opcode_offset(bytes: &[u8]) -> u32 {
    let mut idx = 0usize;
    while idx < bytes.len() {
        match bytes[idx] {
            0xF0 | 0xF2 | 0xF3 | 0x2E | 0x36 | 0x3E | 0x26 | 0x64 | 0x65 | 0x66 | 0x67 => {
                idx += 1;
            }
            0x40..=0x4F => idx += 1,
            0xC5 => return (idx + 2).min(bytes.len().saturating_sub(1)) as u32,
            0xC4 => return (idx + 3).min(bytes.len().saturating_sub(1)) as u32,
            0x62 => return (idx + 4).min(bytes.len().saturating_sub(1)) as u32,
            _ => return idx as u32,
        }
    }
    0
}
