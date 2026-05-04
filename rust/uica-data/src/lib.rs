mod index;
mod manifest;
mod uipack;

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub use index::DataPackIndex;
pub use manifest::{
    load_manifest, load_manifest_pack, load_manifest_runtime, load_manifest_runtime_verified,
    resolve_manifest_pack_path, DataPackManifest, DataPackManifestArchEntry, DataPackManifestError,
    DATAPACK_MANIFEST_FILE_NAME, DATAPACK_MANIFEST_SCHEMA_VERSION,
};
pub use uipack::{
    encode_uipack, load_pack_bytes, load_uipack_bytes, load_uipack_bytes_verified,
    read_uipack_header, read_uipack_header_verified, record_view_to_instruction_record,
    MappedUiPack, MappedUiPackRuntime, UiPackError, UiPackHeader, UiPackPerfView, UiPackPortView,
    UiPackRecordView, UiPackView, UiPackViewIndex, UIPACK_CHECKSUM_FNV1A64, UIPACK_MAGIC,
    UIPACK_VERSION,
};

pub const DATAPACK_SCHEMA_VERSION: &str = "uica-instructions-pack-v2";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DataPack {
    pub schema_version: String,
    /// Python convertXML.py `uArchInfo.allPorts[arch]` generated from port data.
    #[serde(default)]
    pub all_ports: Vec<String>,
    /// Python convertXML.py `uArchInfo.ALUPorts[arch]` generated from AND port data.
    #[serde(default)]
    pub alu_ports: Vec<String>,
    pub instructions: Vec<InstructionRecord>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InstructionRecord {
    pub arch: String,
    pub iform: String,
    pub string: String,
    /// Copy of pack-level generated allPorts for code paths that only receive a row.
    #[serde(default)]
    pub all_ports: Vec<String>,
    /// Copy of pack-level generated ALUPorts for code paths that only receive a row.
    #[serde(default)]
    pub alu_ports: Vec<String>,
    /// XML `locked` row metadata; Python stores this on instrData.
    #[serde(default)]
    pub locked: bool,
    /// XML `allXmlAttributes` row attributes used by Python `xed.matchXMLAttributes()`.
    #[serde(default)]
    pub xml_attrs: BTreeMap<String, String>,
    /// XML `immzero` attribute; Python uses it during XED/XML attribute matching.
    #[serde(default)]
    pub imm_zero: bool,
    pub perf: PerfRecord,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperandRecord {
    /// Operand name from uops.info (e.g. "REG0", "REG1", "REG2" for flags)
    pub name: String,
    /// "reg", "flags", "mem"
    pub r#type: String,
    /// true if this operand is read
    pub read: bool,
    /// true if this operand is written
    pub write: bool,
    /// true if implicit (e.g. CL for SHL)
    pub implicit: bool,
    /// Python flag operand groups represented by this XML operand: "C" and/or "SPAZO".
    #[serde(default)]
    pub flags: Vec<String>,
    /// Flag groups read by this operand, decoded from per-flag XML access values.
    #[serde(default)]
    pub flags_read: Vec<String>,
    /// Flag groups written by this operand, decoded from per-flag XML access values.
    #[serde(default)]
    pub flags_write: Vec<String>,
    /// Memory-address base register name, when provided by XML/XED.
    #[serde(default)]
    pub mem_base: Option<String>,
    /// Memory-address index register name, when provided by XML/XED.
    #[serde(default)]
    pub mem_index: Option<String>,
    /// Memory scale, when provided by XML/XED.
    #[serde(default)]
    pub mem_scale: Option<i32>,
    /// Memory displacement, when provided by XML/XED.
    #[serde(default)]
    pub mem_disp: Option<i64>,
    /// True when this memory operand is AGEN-tagged in source XML.
    #[serde(default)]
    pub is_agen: bool,
    /// Memory role used by computeUopProperties-style modeling: read/write/read_write/agen.
    #[serde(default)]
    pub mem_operand_role: Option<String>,
}

/// Latency from one operand to another, in cycles.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatencyRecord {
    pub start_op: String,
    pub target_op: String,
    pub cycles: i32,
    /// Address-base latency (`cycles_addr` in uops.info / `addr` in Python).
    #[serde(default)]
    pub cycles_addr: Option<i32>,
    /// Address-index latency (`cycles_addr_index` in uops.info / `addrI` in Python).
    #[serde(default)]
    pub cycles_addr_index: Option<i32>,
    /// Memory-data latency (`cycles_mem` in uops.info / `mem` in Python).
    #[serde(default)]
    pub cycles_mem: Option<i32>,
    /// for same-register cases (lat_SR in Python)
    pub cycles_same_reg: Option<i32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PerfVariantRecord {
    #[serde(default)]
    pub uops: Option<i32>,
    #[serde(default)]
    pub retire_slots: Option<i32>,
    #[serde(default)]
    pub uops_mite: Option<i32>,
    #[serde(default)]
    pub uops_ms: Option<i32>,
    #[serde(default)]
    pub tp: Option<f64>,
    #[serde(default)]
    pub ports: Option<BTreeMap<String, i32>>,
    #[serde(default)]
    pub div_cycles: Option<u32>,
    #[serde(default)]
    pub complex_decoder: Option<bool>,
    #[serde(default)]
    pub n_available_simple_decoders: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PerfRecord {
    pub uops: i32,
    pub retire_slots: i32,
    pub uops_mite: i32,
    pub uops_ms: i32,
    pub tp: Option<f64>,
    pub ports: BTreeMap<String, i32>,
    /// Python `perfData` suffix variants, e.g. `_indexed` -> `_I` fields.
    #[serde(default)]
    pub variants: BTreeMap<String, PerfVariantRecord>,
    #[serde(default)]
    pub div_cycles: u32,
    #[serde(default)]
    pub may_be_eliminated: bool,
    #[serde(default)]
    pub complex_decoder: bool,
    #[serde(default)]
    pub n_available_simple_decoders: u32,
    #[serde(default)]
    pub lcp_stall: bool,
    #[serde(default)]
    pub implicit_rsp_change: i32,
    /// Equivalent of Python Instr.canBeUsedByLSD (static metadata path).
    #[serde(default)]
    pub can_be_used_by_lsd: bool,
    #[serde(default)]
    pub cannot_be_in_dsb_due_to_jcc_erratum: bool,
    #[serde(default)]
    pub no_micro_fusion: bool,
    #[serde(default)]
    pub no_macro_fusion: bool,
    /// Python `Instr.macroFusibleWith`: branch instruction strings accepted for macro-fusion.
    #[serde(default)]
    pub macro_fusible_with: Vec<String>,
    /// Operand descriptors (in XML operand-index order).
    #[serde(default)]
    pub operands: Vec<OperandRecord>,
    /// Per-operand-pair latencies (mirrors Python's instr.latencies dict).
    #[serde(default)]
    pub latencies: Vec<LatencyRecord>,
}

pub fn load_pack(path: impl AsRef<Path>) -> Result<DataPack, Box<dyn std::error::Error>> {
    let mapped = MappedUiPack::open(path)?;
    Ok(mapped.view()?.to_data_pack()?)
}

pub fn load_uipack(path: impl AsRef<Path>) -> Result<DataPack, Box<dyn std::error::Error>> {
    let mapped = MappedUiPack::open(path)?;
    Ok(mapped.view()?.to_data_pack()?)
}

pub fn load_uipack_verified(
    path: impl AsRef<Path>,
) -> Result<DataPack, Box<dyn std::error::Error>> {
    let mapped = MappedUiPack::open_verified(path)?;
    Ok(mapped.view()?.to_data_pack()?)
}
