//! Simulator types mirroring `uiCA.py` classes.
//!
//! Ownership: Python uses shared mutable state across Renamer, Scheduler,
//! ReorderBuffer and FrontEnd. We mirror that with `Rc<RefCell<T>>`. This
//! prioritises structural fidelity over idiomatic Rust until parity holds.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

pub type Shared<T> = Rc<RefCell<T>>;

#[inline]
pub fn share<T>(value: T) -> Shared<T> {
    Rc::new(RefCell::new(value))
}

#[inline]
pub fn shared_slice<T>(values: Vec<T>) -> Rc<[T]> {
    Rc::from(values.into_boxed_slice())
}

/// Source of a laminated uop inside the front-end pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UopSource {
    Mite,
    Dsb,
    Ms,
    Lsd,
    Se, // stack sync
}

impl UopSource {
    pub fn as_str(self) -> &'static str {
        match self {
            UopSource::Mite => "MITE",
            UopSource::Dsb => "DSB",
            UopSource::Ms => "MS",
            UopSource::Lsd => "LSD",
            UopSource::Se => "SE",
        }
    }
}

/// Python-like operand identity used by the simulator while string-derived
/// operand fields are being retired. Mirrors `RegOperand`, `FlagOperand`, `MemOperand`,
/// and `PseudoOperand` from `instructions.py`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OperandKey {
    Reg(String),
    Flag(String),
    Mem(u32),
    Pseudo(u64),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MemAddr {
    pub base: Option<String>,
    pub index: Option<String>,
    pub scale: i32,
    pub disp: i64,
    /// Python parity: `RegOperand.isImplicitStackOperand` on RSP address operands
    /// created by STACKPUSH/STACKPOP. FrontEnd stack-sync ignores these operands.
    pub is_implicit_stack_operand: bool,
}

impl OperandKey {
    pub fn from_resolved_name(name: &str) -> Self {
        if let Some(mem_id) = parse_mem_id(name) {
            Self::Mem(mem_id)
        } else if let Some(pseudo_id) = parse_pseudo_id(name) {
            Self::Pseudo(pseudo_id)
        } else if matches!(name, "RFLAGS" | "C" | "SPAZO") {
            Self::Flag(name.to_string())
        } else {
            Self::Reg(crate::x64::get_canonical_reg(name))
        }
    }
}

fn parse_mem_id(name: &str) -> Option<u32> {
    name.strip_prefix("__M_").and_then(|id| id.parse().ok())
}

fn parse_pseudo_id(name: &str) -> Option<u64> {
    if !name.starts_with("__") {
        return None;
    }
    if let Some(id_text) = name.strip_prefix("__P_") {
        return id_text.parse().ok();
    }

    // Stable fallback for any future pseudo namespace. Python uses object
    // identity; Rust needs a deterministic key across one run.
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    Some(hasher.finish())
}

/// Stable per-uop properties derived from instruction data.
/// Mirrors `UopProperties` in `uiCA.py`. Only fields needed by the simulator
/// are modelled; fillers will be expanded as the port progresses.
#[derive(Clone, Debug, Default)]
pub struct UopProperties {
    pub possible_ports: Rc<[String]>,
    pub div_cycles: u32,
    pub is_load_uop: bool,
    pub is_store_address_uop: bool,
    pub is_store_data_uop: bool,
    pub is_first_uop_of_instr: bool,
    pub is_last_uop_of_instr: bool,
    pub is_reg_merge_uop: bool,
    pub is_serializing_instr: bool,
    // Legacy string fields retained while consumers migrate to OperandKey.
    pub input_reg_operands: Rc<[String]>,
    pub output_reg_operands: Rc<[String]>,
    pub may_be_eliminated: bool,
    pub latencies: BTreeMap<String, u32>,
    pub input_operands: Rc<[OperandKey]>,
    /// Python parity: `Instr.inputRegOperands`, excluding memory address
    /// operands. Used for AbstractValueGenerator, not rename dependencies.
    pub instr_input_operands: Rc<[OperandKey]>,
    pub output_operands: Rc<[OperandKey]>,
    pub latencies_by_operand: BTreeMap<OperandKey, u32>,
    // Instruction reference for trace/model output.
    pub instr_tp: Option<u32>,
    pub instr_str: Rc<str>,
    pub immediate: Option<i64>,
    pub is_load_serializing: bool,
    pub is_store_serializing: bool,
    pub mem_addr: Option<MemAddr>,
}

/// Live uop state: lifecycle cycles and scheduler bookkeeping.
/// Mirrors `Uop` in `uiCA.py`.
#[derive(Debug)]
pub struct Uop {
    pub idx: u64,
    /// Python parity: scheduler heap key mirrors `Uop.idx` allocation order.
    /// Reg-merge uops are stored early in Rust but Python creates them in
    /// `Renamer.cycle()`, so renamer assigns this key when injecting them.
    pub queue_idx: u64,
    pub prop: UopProperties,
    pub actual_port: Option<String>,
    pub eliminated: bool,
    pub ready_for_dispatch: Option<u32>,
    pub dispatched: Option<u32>,
    pub executed: Option<u32>,
    pub lat_reduced_due_to_fast_ptr_chasing: bool,
    pub renamed_input_operands: Vec<Shared<RenamedOperand>>,
    pub renamed_output_operands: Vec<Shared<RenamedOperand>>,
    pub store_buffer_entry: Option<Shared<StoreBufferEntry>>,
    pub fused_uop_idx: Option<u64>,
    pub instr_instance_idx: u64,
}

#[derive(Clone, Debug)]
pub struct FusedUop {
    pub idx: u64,
    pub unfused_uop_idxs: Vec<u64>,
    pub laminated_uop_idx: Option<u64>,
    pub issued: Option<u32>,
    pub retired: Option<u32>,
    pub retire_idx: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct LaminatedUop {
    pub idx: u64,
    pub fused_uop_idxs: Vec<u64>,
    pub added_to_idq: Option<u32>,
    pub uop_source: Option<UopSource>,
    pub instr_instance_idx: u64,
}

pub type AbstractValueKey = (u64, i64);

#[derive(Debug)]
pub struct StoreBufferEntry {
    pub key: Option<(Option<AbstractValueKey>, Option<AbstractValueKey>, i32, i64)>,
    pub address_ready_cycle: Option<u32>,
    pub data_ready_cycle: Option<u32>,
    pub uop_idxs: Vec<u64>,
}

#[derive(Debug, Clone)]
pub struct RenamedOperand {
    pub ready: Option<i64>, // use i64 since Python uses -1 for "initial"
    pub uop_idx: Option<u64>,
    pub latency: Option<u32>,
    pub operand: Option<OperandKey>,
    pub identity: u64,
}

impl Default for RenamedOperand {
    fn default() -> Self {
        Self::new()
    }
}

static RENAMED_OPERAND_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub fn next_renamed_operand_identity() -> u64 {
    RENAMED_OPERAND_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl RenamedOperand {
    pub fn new() -> Self {
        Self {
            ready: Some(-1),
            uop_idx: None,
            latency: None,
            operand: None,
            identity: next_renamed_operand_identity(),
        }
    }

    pub fn get_ready_cycle(&self, storage: &super::uop_storage::UopStorage) -> Option<u32> {
        // If ready is already computed, return it
        if let Some(r) = self.ready {
            return if r < 0 { Some(0) } else { Some(r as u32) };
        }

        // Otherwise, compute from uop_idx
        let uop_idx = self.uop_idx?;
        let uop = storage.get_uop(uop_idx)?;
        let dispatched = uop.dispatched?;
        let mut lat = self.latency.unwrap_or(1);
        if uop.lat_reduced_due_to_fast_ptr_chasing {
            lat = lat.saturating_sub(1);
        }

        if uop.prop.is_load_uop {
            if let Some(sb) = &uop.store_buffer_entry {
                let sb = sb.borrow();
                let address_ready = sb.address_ready_cycle?;
                let data_ready = sb.data_ready_cycle?;
                let mem_ready = address_ready.max(data_ready) + 4;
                return Some((dispatched + lat).max(mem_ready));
            }
        }

        Some(dispatched + lat)
    }
}

/// Immutable decoded instruction data shared by runtime instances.
/// Initial scaffold keeps duplicated fields on `InstrInstance`; later phases
/// will migrate hot paths to read through `template_id`.
#[derive(Debug, Clone)]
pub struct InstrTemplate {
    pub instr_id: u32,
    pub size: u32,
    pub pos_nominal_opcode: u32,
    pub mnemonic: Rc<str>,
    pub disasm: Rc<str>,
    pub opcode_hex: Rc<str>,
    pub input_regs: Rc<[String]>,
    pub output_regs: Rc<[String]>,
    pub reads_flags: bool,
    pub writes_flags: bool,
    pub has_memory_read: bool,
    pub has_memory_write: bool,
    pub mem_addrs: Rc<[MemAddr]>,
    pub immediate: Option<i64>,
    pub decoded_iform: Rc<str>,
    pub iform_signature: Rc<str>,
    pub max_op_size_bytes: u8,
    pub uses_high8_reg: bool,
    pub explicit_reg_operands: Rc<[String]>,
    pub agen: Option<Rc<str>>,
    pub xml_attrs: Rc<BTreeMap<String, String>>,
    pub is_branch_instr: bool,
    pub is_serializing_instr: bool,
    pub is_load_serializing: bool,
    pub is_store_serializing: bool,
    pub implicit_rsp_change: i32,
}

impl InstrTemplate {
    pub fn from_instance(inst: &InstrInstance) -> Self {
        Self {
            instr_id: inst.instr_id,
            size: inst.size,
            pos_nominal_opcode: inst.pos_nominal_opcode,
            mnemonic: inst.mnemonic.clone(),
            disasm: inst.disasm.clone(),
            opcode_hex: inst.opcode_hex.clone(),
            input_regs: inst.input_regs.clone(),
            output_regs: inst.output_regs.clone(),
            reads_flags: inst.reads_flags,
            writes_flags: inst.writes_flags,
            has_memory_read: inst.has_memory_read,
            has_memory_write: inst.has_memory_write,
            mem_addrs: inst.mem_addrs.clone(),
            immediate: inst.immediate,
            decoded_iform: inst.decoded_iform.clone(),
            iform_signature: inst.iform_signature.clone(),
            max_op_size_bytes: inst.max_op_size_bytes,
            uses_high8_reg: inst.uses_high8_reg,
            explicit_reg_operands: inst.explicit_reg_operands.clone(),
            agen: inst.agen.clone(),
            xml_attrs: inst.xml_attrs.clone(),
            is_branch_instr: inst.is_branch_instr,
            is_serializing_instr: inst.is_serializing_instr,
            is_load_serializing: inst.is_load_serializing,
            is_store_serializing: inst.is_store_serializing,
            implicit_rsp_change: inst.implicit_rsp_change,
        }
    }
}

/// Runtime instance of an instruction (one per loop iteration).
/// Mirrors `InstrInstance` in `uiCA.py`.
#[derive(Debug, Clone)]
pub struct InstrInstance {
    pub idx: u64,
    pub instr_id: u32,
    pub template_id: usize,
    pub rnd: u32,

    // Physical properties (from Instr)
    pub address: u32,
    pub size: u32,
    pub pos_nominal_opcode: u32,
    pub mnemonic: Rc<str>,
    pub disasm: Rc<str>,
    pub opcode_hex: Rc<str>,

    // Operand info from decoder
    pub input_regs: Rc<[String]>,
    pub output_regs: Rc<[String]>,
    pub reads_flags: bool,
    pub writes_flags: bool,
    pub has_memory_read: bool,
    pub has_memory_write: bool,
    pub mem_addrs: Rc<[MemAddr]>,
    pub immediate: Option<i64>,
    /// Exact XED iform decoded for this instruction.
    pub decoded_iform: Rc<str>,
    /// Iform-style operand signature for instruction-record disambiguation.
    pub iform_signature: Rc<str>,
    /// Max operand register size bytes (0=unknown); used for record disambiguation.
    pub max_op_size_bytes: u8,
    /// True when explicit operand uses AH/BH/CH/DH; disambiguates R8h/R8l records.
    pub uses_high8_reg: bool,
    /// Explicit register operands in instruction operand order; mirrors XED attrs.
    pub explicit_reg_operands: Rc<[String]>,
    /// XED `agen` attribute for LEA addressing forms (e.g. B_IS_D8).
    pub agen: Option<Rc<str>>,
    /// XED/XML match attributes used by Python `xed.matchXMLAttributes()`.
    pub xml_attrs: Rc<BTreeMap<String, String>>,

    // Decoder properties
    pub is_branch_instr: bool,
    pub complex_decoder: bool,
    pub n_available_simple_decoders: u32,
    pub lcp_stall: bool,

    // Macro fusion annotations (set during instruction building)
    pub macro_fused_with_prev_instr: bool,
    pub macro_fused_with_next_instr: bool,
    pub is_macro_fusible_with_next: bool,
    pub macro_fusible_with: Rc<[String]>,
    pub is_last_decoded_instr: bool,

    // Uop counts
    pub uops_mite: u32,
    pub uops_ms: u32,
    pub div_cycles: u32,
    pub retire_slots: u32,
    pub instr_tp: Option<u32>,
    pub instr_str: Rc<str>,
    pub implicit_rsp_change: i32,
    pub may_be_eliminated: bool,
    pub is_serializing_instr: bool,
    pub is_load_serializing: bool,
    pub is_store_serializing: bool,
    pub cannot_be_in_dsb_due_to_jcc_erratum: bool,
    pub no_micro_fusion: bool,
    pub no_macro_fusion: bool,

    // Lifecycle tracking
    pub predecoded: Option<u32>,
    pub removed_from_iq: Option<u32>,
    pub source: Option<UopSource>,

    // Uop collections (populated later in the pipeline)
    pub laminated_uops: Vec<u64>,
    pub reg_merge_uops: Vec<u64>,
    pub stack_sync_uops: Vec<u64>,

    /// LSD eligibility refined from instruction metadata during front-end setup.
    pub can_be_used_by_lsd: bool,
}

impl InstrInstance {
    pub fn new(
        idx: u64,
        instr_id: u32,
        rnd: u32,
        address: u32,
        size: u32,
        mnemonic: String,
        disasm: String,
    ) -> Self {
        Self {
            idx,
            instr_id,
            template_id: instr_id as usize,
            rnd,
            address,
            size,
            pos_nominal_opcode: 0,
            mnemonic: mnemonic.into(),
            disasm: disasm.into(),
            opcode_hex: Rc::from(""),
            input_regs: shared_slice(Vec::new()),
            output_regs: shared_slice(Vec::new()),
            reads_flags: false,
            writes_flags: false,
            has_memory_read: false,
            has_memory_write: false,
            mem_addrs: shared_slice(Vec::new()),
            immediate: None,
            decoded_iform: Rc::from(""),
            iform_signature: Rc::from(""),
            max_op_size_bytes: 0,
            uses_high8_reg: false,
            explicit_reg_operands: shared_slice(Vec::new()),
            agen: None,
            xml_attrs: Rc::new(BTreeMap::new()),
            is_branch_instr: false,
            complex_decoder: false,
            n_available_simple_decoders: 4,
            lcp_stall: false,
            macro_fused_with_prev_instr: false,
            macro_fused_with_next_instr: false,
            is_macro_fusible_with_next: false,
            macro_fusible_with: shared_slice(Vec::new()),
            is_last_decoded_instr: false,
            uops_mite: 1, // default to 1 uop
            uops_ms: 0,
            div_cycles: 0,
            retire_slots: 1,
            instr_tp: None,
            instr_str: Rc::from(""),
            implicit_rsp_change: 0,
            may_be_eliminated: false,
            is_serializing_instr: false,
            is_load_serializing: false,
            is_store_serializing: false,
            cannot_be_in_dsb_due_to_jcc_erratum: false,
            no_micro_fusion: false,
            no_macro_fusion: false,
            predecoded: None,
            removed_from_iq: None,
            source: None,
            laminated_uops: Vec::new(),
            reg_merge_uops: Vec::new(),
            stack_sync_uops: Vec::new(),
            can_be_used_by_lsd: true, // default true, refined from instruction metadata
        }
    }
}

/// Recompute macro-fusion pair flags and decoder-boundary flags after
/// instruction metadata is available.
pub fn recompute_macro_fusion_and_is_last(instances: &mut [InstrInstance]) {
    if instances.is_empty() {
        return;
    }

    for inst in instances.iter_mut() {
        inst.macro_fused_with_prev_instr = false;
        inst.macro_fused_with_next_instr = false;
        inst.is_last_decoded_instr = false;
        if inst.no_macro_fusion {
            inst.is_macro_fusible_with_next = false;
        }
    }

    for i in 1..instances.len() {
        let branch = &instances[i];
        if !branch.is_branch_instr || branch.no_macro_fusion || branch.address.is_multiple_of(64) {
            continue;
        }
        if !instances[i - 1].no_macro_fusion
            && instances[i - 1]
                .macro_fusible_with
                .iter()
                .any(|instr_str| instr_str.as_str() == branch.instr_str.as_ref())
        {
            instances[i].macro_fused_with_prev_instr = true;
            instances[i - 1].macro_fused_with_next_instr = true;
        }
    }

    // Python parity: `Instr.isLastDecodedInstr()` is true for the last
    // instruction, and also for the next-to-last instruction when it is
    // macro-fused with the final branch. The macro-fused branch has no uops,
    // so DSB/summary code observes the previous instruction as last decoded.
    let last_idx = instances.len() - 1;
    instances[last_idx].is_last_decoded_instr = true;
    if last_idx > 0 && instances[last_idx - 1].macro_fused_with_next_instr {
        instances[last_idx - 1].is_last_decoded_instr = true;
    }
}

/// Build instruction instances from decoded bytes with macro fusion detection.
///
/// Macro fusion rules:
/// - DEC/CMP/SUB/TEST-style instructions followed by a conditional branch can macro-fuse
/// - Fusion does not happen when the jump is at the start of a 64-byte cache line
///
/// This function mirrors the logic in `getInstructions` from instructions.py,
/// focusing only on the fields needed for PreDecoder and Decoder.
pub fn build_instruction_templates(instances: &[InstrInstance]) -> Vec<InstrTemplate> {
    instances.iter().map(InstrTemplate::from_instance).collect()
}

pub fn build_instruction_instances(
    decoded: &[uica_decode_ir::DecodedInstruction],
    alignment_offset: u32,
) -> Vec<InstrInstance> {
    let mut instances: Vec<InstrInstance> = Vec::new();
    let mut next_addr = alignment_offset;
    let mut instance_idx = 0u64;

    #[allow(clippy::explicit_counter_loop)]
    for (instr_id, dec) in decoded.iter().enumerate() {
        let addr = next_addr;
        next_addr += dec.len;

        let mut inst = InstrInstance::new(
            instance_idx,
            instr_id as u32,
            0, // rnd = 0 for first iteration
            addr,
            dec.len,
            dec.mnemonic.clone(),
            dec.disasm.clone(),
        );
        inst.opcode_hex = Rc::from(
            dec.bytes
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<String>(),
        );
        inst.pos_nominal_opcode = dec.pos_nominal_opcode;
        inst.lcp_stall = dec.has_66_prefix && dec.immediate_width_bits == 16;

        // Copy decoder-derived operand info
        inst.input_regs = shared_slice(dec.input_regs.clone());
        inst.output_regs = shared_slice(dec.output_regs.clone());
        inst.reads_flags = dec.reads_flags;
        inst.writes_flags = dec.writes_flags;
        inst.has_memory_read = dec.has_memory_read;
        inst.has_memory_write = dec.has_memory_write;
        inst.mem_addrs = shared_slice(
            dec.mem_addrs
                .iter()
                .map(|m| MemAddr {
                    base: m.base.clone(),
                    index: m.index.clone(),
                    scale: m.scale,
                    disp: m.disp,
                    is_implicit_stack_operand: m.is_implicit_stack_operand,
                })
                .collect(),
        );
        // Python parity: `getInstructions()` computes `implicitRSPChange`
        // from decoded XED STACKPUSH/STACKPOP operands before applying XML
        // performance rows. Preserve decoded stack-pointer effects here.
        inst.implicit_rsp_change = dec.implicit_rsp_change;
        inst.immediate = dec.immediate;
        inst.decoded_iform = Rc::from(dec.iform.clone());
        inst.iform_signature = Rc::from(dec.iform_signature.clone());
        inst.max_op_size_bytes = dec.max_op_size_bytes;
        inst.uses_high8_reg = dec.uses_high8_reg;
        inst.explicit_reg_operands = shared_slice(dec.explicit_reg_operands.clone());
        inst.agen = dec.agen.as_ref().map(|agen| Rc::from(agen.as_str()));
        inst.xml_attrs = Rc::new(dec.xml_attrs.clone());

        // Detect conditional branches for decoder and macro-fusion handling.
        inst.is_branch_instr = is_conditional_branch(&dec.mnemonic);
        inst.is_serializing_instr = is_serializing_instr(&dec.mnemonic);
        inst.is_load_serializing = is_load_serializing(&dec.mnemonic);
        inst.is_store_serializing = is_store_serializing(&dec.mnemonic);

        instances.push(inst);
        instance_idx += 1;
    }

    recompute_macro_fusion_and_is_last(&mut instances);

    instances
}

fn is_conditional_branch(mnemonic: &str) -> bool {
    matches!(
        mnemonic,
        "jo" | "jno"
            | "jb"
            | "jnb"
            | "jz"
            | "jnz"
            | "jbe"
            | "jnbe"
            | "js"
            | "jns"
            | "jp"
            | "jnp"
            | "jl"
            | "jnl"
            | "jle"
            | "jnle"
            | "jc"
            | "jnc"
            | "je"
            | "jne"
            | "ja"
            | "jae"
            | "jg"
            | "jge"
    )
}

fn is_serializing_instr(mnemonic: &str) -> bool {
    matches!(
        mnemonic,
        "lfence"
            | "cpuid"
            | "iret"
            | "iretd"
            | "rsm"
            | "invd"
            | "invept"
            | "invlpg"
            | "invvpid"
            | "lgdt"
            | "lidt"
            | "lldt"
            | "ltr"
            | "wbinvd"
            | "wrmsr"
    )
}

fn is_load_serializing(mnemonic: &str) -> bool {
    matches!(mnemonic, "mfence" | "lfence")
}

fn is_store_serializing(mnemonic: &str) -> bool {
    matches!(mnemonic, "mfence" | "sfence")
}

#[cfg(test)]
mod tests {
    use super::{build_instruction_instances, recompute_macro_fusion_and_is_last, shared_slice};

    #[test]
    fn macro_fusion_uses_uipack_branch_strings() {
        let decoded = uica_decoder::decode_raw(&[0x48, 0x3b, 0x07, 0x70, 0x02]).unwrap();
        let mut instances = build_instruction_instances(&decoded, 0);
        instances[0].instr_str = "CMP (R64, M64)".into();
        instances[0].macro_fusible_with = shared_slice(vec!["JZ (Rel8)".to_string()]);
        instances[0].is_macro_fusible_with_next = true;
        instances[1].instr_str = "JO (Rel8)".into();
        recompute_macro_fusion_and_is_last(&mut instances);
        assert!(!instances[0].macro_fused_with_next_instr);
        assert!(!instances[1].macro_fused_with_prev_instr);

        instances[1].instr_str = "JZ (Rel8)".into();
        recompute_macro_fusion_and_is_last(&mut instances);
        assert!(instances[0].macro_fused_with_next_instr);
        assert!(instances[1].macro_fused_with_prev_instr);
    }

    #[test]
    fn build_instruction_instances_assigns_template_ids() {
        let decoded = uica_decoder::decode_raw(&[0x48, 0x01, 0xd8, 0x48, 0xff, 0xc0]).unwrap();
        let instances = build_instruction_instances(&decoded, 0);
        let templates = super::build_instruction_templates(&instances);

        assert_eq!(instances.len(), 2);
        assert_eq!(templates.len(), 2);
        assert_eq!(instances[0].template_id, 0);
        assert_eq!(instances[1].template_id, 1);
        assert_eq!(templates[0].mnemonic, instances[0].mnemonic);
        assert_eq!(templates[1].decoded_iform, instances[1].decoded_iform);
    }

    #[test]
    fn lfence_is_serializing_like_python_getinstructions() {
        let decoded = uica_decoder::decode_raw(&[0x0f, 0xae, 0xe8]).unwrap();
        let instances = build_instruction_instances(&decoded, 0);

        assert!(instances[0].is_serializing_instr);
        assert!(instances[0].is_load_serializing);
        assert!(!instances[0].is_store_serializing);
    }
}
