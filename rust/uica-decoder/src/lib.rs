use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use iced_x86::{
    Decoder, DecoderError, DecoderOptions, FormatMnemonicOptions, Formatter,
    InstructionInfoFactory, NasmFormatter, OpAccess, OpKind, Register,
};
use object::{Object, ObjectSection};

/// Operand kind signature for matcher disambiguation. Mirrors the middle
/// of a uops.info iform (e.g. `ADC_GPRv_GPRv_11` → iform_signature = `GPRv_GPRv`).
/// The decoder produces this so the matcher can prefer records whose iform
/// contains the same sequence, avoiding false matches against (AL, IMM8)
/// etc. for a given mnemonic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedMemAddr {
    pub base: Option<String>,
    pub index: Option<String>,
    pub scale: i32,
    pub disp: i64,
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
    pub immediate: Option<i64>,
    /// Operand-kind signature used for iform matching (e.g. `GPRv_GPRv`).
    pub iform_signature: String,
    /// Max operand register size in bytes (8=R64, 4=R32, 2=R16, 1=R8, 0=unknown).
    pub max_op_size_bytes: u8,
}

pub fn decode_raw(bytes: &[u8]) -> Result<Vec<DecodedInstruction>> {
    let mut decoder = Decoder::with_ip(64, bytes, 0, DecoderOptions::NONE);
    let mut formatter = NasmFormatter::new();
    // Python disassembly uses "add rax, rbx" with a space after the comma.
    // Match that so the JSON `asm` field compares byte-for-byte.
    formatter
        .options_mut()
        .set_space_after_operand_separator(true);
    let mut info_factory = InstructionInfoFactory::new();
    let mut instructions = Vec::new();

    while decoder.can_decode() {
        let offset = decoder.position();
        let instruction = decoder.decode();

        match decoder.last_error() {
            DecoderError::None => {}
            DecoderError::NoMoreBytes => {
                bail!("truncated instruction stream at byte offset {offset}")
            }
            DecoderError::InvalidInstruction => {
                bail!("invalid instruction at byte offset {offset}")
            }
            _ => bail!("decoder error at byte offset {offset}"),
        }

        let mut mnemonic = String::new();
        formatter.format_mnemonic_options(
            &instruction,
            &mut mnemonic,
            FormatMnemonicOptions::NO_PREFIXES,
        );

        let mut disasm = String::new();
        formatter.format(&instruction, &mut disasm);

        let len = instruction.len();
        let instr_bytes = bytes
            .get(offset..offset + len)
            .map(|s| s.to_vec())
            .unwrap_or_default();

        // Extract register and memory operand info
        let info = info_factory.info(&instruction);
        let mut input_regs = Vec::new();
        let mut output_regs = Vec::new();

        // For VEX instructions with 3+ operands, iced-x86's used_registers()
        // sometimes conflates operand roles. Use op_access() on each explicit
        // operand directly for better accuracy.
        if instruction.op_count() >= 3 {
            for i in 0..instruction.op_count() {
                if instruction.op_kind(i) != OpKind::Register {
                    continue;
                }
                let reg_str = format_register(instruction.op_register(i));
                match info.op_access(i) {
                    OpAccess::Read | OpAccess::CondRead => input_regs.push(reg_str),
                    // CondWrite: conditional write preserves the old value when
                    // condition is false (e.g. cmov dest reg). Treat as ReadWrite
                    // to match Python/XED model which includes dest as input.
                    OpAccess::CondWrite => {
                        input_regs.push(reg_str.clone());
                        output_regs.push(reg_str);
                    }
                    OpAccess::Write => output_regs.push(reg_str),
                    OpAccess::ReadWrite | OpAccess::ReadCondWrite => {
                        input_regs.push(reg_str.clone());
                        output_regs.push(reg_str);
                    }
                    _ => {}
                }
            }
        } else {
            for used_reg in info.used_registers() {
                let reg_str = format_register(used_reg.register());
                match used_reg.access() {
                    OpAccess::Read | OpAccess::CondRead => {
                        input_regs.push(reg_str);
                    }
                    // CondWrite: conditional write preserves old value (e.g. cmov).
                    // Treat as ReadWrite to match Python/XED semantics.
                    OpAccess::CondWrite => {
                        input_regs.push(reg_str.clone());
                        output_regs.push(reg_str);
                    }
                    OpAccess::Write => {
                        output_regs.push(reg_str);
                    }
                    OpAccess::ReadWrite | OpAccess::ReadCondWrite => {
                        input_regs.push(reg_str.clone());
                        output_regs.push(reg_str);
                    }
                    _ => {}
                }
            }
        }

        let reads_flags = instruction.rflags_read() != 0;
        let writes_flags = instruction.rflags_written() != 0;

        let mut has_memory_read = false;
        let mut has_memory_write = false;
        let mut mem_addrs = Vec::new();
        for used_mem in info.used_memory() {
            match used_mem.access() {
                OpAccess::Read | OpAccess::CondRead => {
                    has_memory_read = true;
                }
                OpAccess::Write | OpAccess::CondWrite => {
                    has_memory_write = true;
                }
                OpAccess::ReadWrite | OpAccess::ReadCondWrite => {
                    has_memory_read = true;
                    has_memory_write = true;
                }
                _ => {}
            }
            let base = format_register(used_mem.base()).to_ascii_uppercase();
            let index = format_register(used_mem.index()).to_ascii_uppercase();
            mem_addrs.push(DecodedMemAddr {
                base: if base == "NONE" { None } else { Some(base) },
                index: if index == "NONE" { None } else { Some(index) },
                scale: used_mem.scale() as i32,
                disp: used_mem.displacement() as i64,
            });
        }

        let immediate = first_immediate(&instruction);
        let iform_signature = build_iform_signature(&instruction);
        // Max register operand size — used by the matcher to pick the right
        // R16/R32/R64 variant from records sharing the same iform prefix.
        let max_op_size_bytes = (0..instruction.op_count())
            .filter_map(|i| {
                if instruction.op_kind(i) == OpKind::Register {
                    Some(instruction.op_register(i).info().size() as u8)
                } else {
                    None
                }
            })
            .max()
            .unwrap_or(0);

        instructions.push(DecodedInstruction {
            ip: instruction.ip(),
            len: len as u32,
            mnemonic,
            disasm,
            pos_nominal_opcode: nominal_opcode_offset(&instr_bytes),
            bytes: instr_bytes,
            input_regs,
            output_regs,
            reads_flags,
            writes_flags,
            has_memory_read,
            has_memory_write,
            mem_addrs,
            immediate,
            iform_signature,
            max_op_size_bytes,
        });
    }

    Ok(instructions)
}

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

fn first_immediate(instruction: &iced_x86::Instruction) -> Option<i64> {
    for i in 0..instruction.op_count() {
        let imm = match instruction.op_kind(i) {
            OpKind::Immediate8 | OpKind::Immediate8_2nd => instruction.immediate8() as i8 as i64,
            OpKind::Immediate8to16 | OpKind::Immediate8to32 | OpKind::Immediate8to64 => {
                instruction.immediate8() as i8 as i64
            }
            OpKind::Immediate16 => instruction.immediate16() as i16 as i64,
            OpKind::Immediate32 => instruction.immediate32() as i32 as i64,
            OpKind::Immediate32to64 => instruction.immediate32to64(),
            OpKind::Immediate64 => instruction.immediate64() as i64,
            _ => continue,
        };
        return Some(imm);
    }
    None
}

fn format_register(reg: Register) -> String {
    format!("{:?}", reg).to_ascii_uppercase()
}

/// Construct a uops.info-style operand signature by walking the instruction's
/// operand list. Useful for picking the right DataPack record among many
/// sharing a mnemonic (e.g. ADC has variants for AL/IMM8, GPR8, GPRv, MEM,
/// …).
fn build_iform_signature(instruction: &iced_x86::Instruction) -> String {
    let mut parts = Vec::new();
    for i in 0..instruction.op_count() {
        let kind = instruction.op_kind(i);
        let part = match kind {
            OpKind::Register => register_kind_tag(instruction.op_register(i)),
            OpKind::Memory => memory_kind_tag(instruction),
            OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64 => "RELBRz",
            OpKind::FarBranch16 | OpKind::FarBranch32 => "PTRb",
            OpKind::Immediate8
            | OpKind::Immediate8to16
            | OpKind::Immediate8to32
            | OpKind::Immediate8to64
            | OpKind::Immediate8_2nd => {
                // uops.info uses suffix _ONE for shift/rotate with imm=1.
                // iced-x86 represents both encodings as Immediate8.
                if instruction.immediate8() == 1 {
                    "ONE"
                } else {
                    "IMMb"
                }
            }
            OpKind::Immediate16 => "IMMw",
            OpKind::Immediate32 | OpKind::Immediate32to64 => "IMMz",
            OpKind::Immediate64 => "IMMq",
            _ => "UNK",
        };
        parts.push(part.to_string());
    }
    parts.join("_")
}

fn register_kind_tag(reg: Register) -> &'static str {
    // Use the size/class of the register in uops.info shorthand.
    // GPR64 -> GPRv, GPR32 -> GPRy, GPR16 -> GPRw, GPR8 -> GPR8, XMM -> XMMps,
    // YMM -> YMMqq, ZMM -> ZMMqq, KReg -> MASKm, MMX -> MMX
    let info = reg.info();
    let size = info.size();
    if reg.is_gpr() {
        match size {
            // uops.info uses GPRv for 16, 32, and 64-bit register operands
            // when the instruction's size is determined by the operand-size
            // prefix (not a fixed R64). GPRy (used by some compilers for R32)
            // does not appear in uops.info iforms; collapse it to GPRv here.
            8 | 4 | 2 => "GPRv",
            1 => "GPR8",
            _ => "GPR",
        }
    } else if reg.is_xmm() {
        "XMMdq" // uops.info uses XMMdq for 128-bit XMM in iforms
    } else if reg.is_ymm() {
        "YMMqq"
    } else if reg.is_zmm() {
        "ZMMqq"
    } else if reg.is_k() {
        "MASKm"
    } else if reg.is_mm() {
        "MMX"
    } else {
        "REG"
    }
}

fn memory_kind_tag(instruction: &iced_x86::Instruction) -> &'static str {
    let size = instruction.memory_size().size();
    match size {
        1 => "MEMb",
        2 => "MEMw",
        4 => "MEMd",
        8 => "MEMv",
        16 => "MEMdq",
        32 => "MEMqq",
        64 => "MEMoq",
        _ => "MEM",
    }
}

pub fn extract_text_from_object(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    let path = path.as_ref();
    let bytes =
        fs::read(path).with_context(|| format!("failed to read object file {}", path.display()))?;
    let file = object::File::parse(&*bytes)
        .with_context(|| format!("failed to parse object file {}", path.display()))?;
    let section = file
        .section_by_name(".text")
        .context("object missing .text section")?;
    let data = section
        .uncompressed_data()
        .context("failed to read .text section")?;
    Ok(data.into_owned())
}
