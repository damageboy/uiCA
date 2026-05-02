// UiPack v1 layout:
// - 64-byte little-endian header with version, checksum, section offsets/counts.
// - String section stores [u32 len][utf8 bytes] entries, addressed by offsets.
// - Record section stores fixed-width instruction records.
// - Port section stores fixed-width port/count pairs referenced by record ranges.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

#[cfg(not(target_family = "wasm"))]
use std::fs::File;

use crate::{
    DataPack, InstructionRecord, LatencyRecord, OperandRecord, PerfRecord, PerfVariantRecord,
};

pub const UIPACK_MAGIC: [u8; 8] = *b"UIPACK\0\0";
pub const UIPACK_VERSION: u16 = 5;
pub const UIPACK_CHECKSUM_FNV1A64: u16 = 1;

const UIPACK_HEADER_SIZE: usize = 64;
const UIPACK_RECORD_SIZE: usize = 88;
const UIPACK_PORT_ENTRY_SIZE: usize = 8;
const UIPACK_ALIGNMENT: usize = 8;
const CHECKSUM_OFFSET: usize = 24;

const FNV1A64_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV1A64_PRIME: u64 = 0x100000001b3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiPackHeader {
    pub version: u16,
    pub header_size: u16,
    pub checksum_kind: u16,
    pub file_len: u64,
    pub checksum: u64,
    pub arch_offset: u32,
    pub strings_offset: u32,
    pub strings_size: u32,
    pub records_offset: u32,
    pub records_count: u32,
    pub ports_offset: u32,
    pub ports_count: u32,
    pub schema_offset: u32,
}

#[derive(Debug)]
pub enum UiPackError {
    Io(std::io::Error),
    Json(serde_json::Error),
    InvalidFormat(String),
}

impl fmt::Display for UiPackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::Json(err) => write!(f, "JSON parse error: {err}"),
            Self::InvalidFormat(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for UiPackError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Json(err) => Some(err),
            Self::InvalidFormat(_) => None,
        }
    }
}

impl From<std::io::Error> for UiPackError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for UiPackError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

pub struct MappedUiPack {
    bytes: UiPackBytes,
}

#[cfg(not(target_family = "wasm"))]
enum UiPackBytes {
    Mmap(memmap2::Mmap),
    Owned(Box<[u8]>),
}

#[cfg(target_family = "wasm")]
enum UiPackBytes {
    Owned(Box<[u8]>),
}

#[derive(Clone, Copy, Debug)]
pub struct UiPackView<'a> {
    bytes: &'a [u8],
    header: UiPackHeader,
    strings: &'a [u8],
    arch: &'a str,
    schema_version: &'a str,
}

#[derive(Clone, Copy, Debug)]
pub struct UiPackRecordView<'a> {
    view: &'a UiPackView<'a>,
    index: u32,
    entry: RecordEntry,
    iform: &'a str,
    string: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiPackPerfView {
    uops: i32,
    retire_slots: i32,
    uops_mite: i32,
    uops_ms: i32,
    tp: Option<f64>,
    div_cycles: u32,
    may_be_eliminated: bool,
    complex_decoder: bool,
    n_available_simple_decoders: u32,
    lcp_stall: bool,
    implicit_rsp_change: i32,
    can_be_used_by_lsd: bool,
    cannot_be_in_dsb_due_to_jcc_erratum: bool,
    no_micro_fusion: bool,
    no_macro_fusion: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiPackPortView<'a> {
    key: &'a str,
    count: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiPackViewIndex {
    by_mnemonic: BTreeMap<String, Vec<u32>>,
    by_iform: BTreeMap<String, Vec<u32>>,
    empty: Vec<u32>,
}

impl MappedUiPack {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, UiPackError> {
        #[cfg(not(target_family = "wasm"))]
        {
            let file = File::open(path)?;
            let mmap = unsafe { memmap2::MmapOptions::new().map(&file)? };
            Ok(Self {
                bytes: UiPackBytes::Mmap(mmap),
            })
        }

        #[cfg(target_family = "wasm")]
        {
            Ok(Self::from_bytes(std::fs::read(path)?))
        }
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self {
            bytes: UiPackBytes::Owned(bytes.into_boxed_slice()),
        }
    }

    pub fn bytes(&self) -> &[u8] {
        match &self.bytes {
            #[cfg(not(target_family = "wasm"))]
            UiPackBytes::Mmap(bytes) => bytes,
            UiPackBytes::Owned(bytes) => bytes,
        }
    }

    pub fn view(&self) -> Result<UiPackView<'_>, UiPackError> {
        UiPackView::from_bytes(self.bytes())
    }
}

impl<'a> UiPackView<'a> {
    pub fn from_bytes(bytes: &'a [u8]) -> Result<Self, UiPackError> {
        let header = read_uipack_header(bytes)?;
        let strings_start = usize::try_from(header.strings_offset).unwrap();
        let strings_end = strings_start + usize::try_from(header.strings_size).unwrap();
        let strings = &bytes[strings_start..strings_end];
        let arch = read_string_ref(strings, header.arch_offset)?;
        let schema_version = read_string_ref(strings, header.schema_offset)?;

        Ok(Self {
            bytes,
            header,
            strings,
            arch,
            schema_version,
        })
    }

    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    pub fn header(&self) -> UiPackHeader {
        self.header
    }

    pub fn arch(&self) -> &'a str {
        self.arch
    }

    pub fn schema_version(&self) -> &'a str {
        self.schema_version
    }

    pub fn record_count(&self) -> u32 {
        self.header.records_count
    }

    pub fn resolve_string(&self, offset: u32) -> Result<&'a str, UiPackError> {
        read_string_ref(self.strings, offset)
    }

    pub fn record(&'a self, index: u32) -> Result<UiPackRecordView<'a>, UiPackError> {
        if index >= self.header.records_count {
            return Err(UiPackError::InvalidFormat(format!(
                "uipack record index {index} out of bounds"
            )));
        }

        let record_size = record_size_for_version(self.header.version)?;
        let start = usize::try_from(self.header.records_offset).unwrap()
            + usize::try_from(index).unwrap() * record_size;
        let entry = read_record_entry(&self.bytes[start..start + record_size], self.header.version);
        let iform = read_string_ref(self.strings, entry.iform_offset)?;
        let string = read_string_ref(self.strings, entry.string_offset)?;
        validate_record_ports_range(entry, self.header.ports_count)?;

        Ok(UiPackRecordView {
            view: self,
            index,
            entry,
            iform,
            string,
        })
    }

    pub fn to_data_pack(&'a self) -> Result<DataPack, UiPackError> {
        let mut instructions = Vec::with_capacity(usize::try_from(self.record_count()).unwrap());

        for index in 0..self.record_count() {
            let record = self.record(index)?;
            let mut ports = BTreeMap::new();
            for port in record.ports()? {
                if ports.insert(port.key().to_string(), port.count()).is_some() {
                    return Err(UiPackError::InvalidFormat(format!(
                        "duplicate port key '{}' in uipack record",
                        port.key()
                    )));
                }
            }

            instructions.push(InstructionRecord {
                arch: self.arch().to_string(),
                iform: record.iform().to_string(),
                string: record.string().to_string(),
                imm_zero: record.imm_zero(),
                perf: PerfRecord {
                    uops: record.perf().uops(),
                    retire_slots: record.perf().retire_slots(),
                    uops_mite: record.perf().uops_mite(),
                    uops_ms: record.perf().uops_ms(),
                    tp: record.perf().tp(),
                    ports,
                    div_cycles: record.perf().div_cycles(),
                    may_be_eliminated: record.perf().may_be_eliminated(),
                    complex_decoder: record.perf().complex_decoder(),
                    n_available_simple_decoders: record.perf().n_available_simple_decoders(),
                    lcp_stall: record.perf().lcp_stall(),
                    implicit_rsp_change: record.perf().implicit_rsp_change(),
                    can_be_used_by_lsd: record.perf().can_be_used_by_lsd(),
                    cannot_be_in_dsb_due_to_jcc_erratum: record
                        .perf()
                        .cannot_be_in_dsb_due_to_jcc_erratum(),
                    no_micro_fusion: record.perf().no_micro_fusion(),
                    no_macro_fusion: record.perf().no_macro_fusion(),
                    macro_fusible_with: record.macro_fusible_with()?,
                    operands: record.operands()?,
                    latencies: record.latencies()?,
                    variants: record.variants()?,
                },
            });
        }

        Ok(DataPack {
            schema_version: self.schema_version().to_string(),
            instructions,
        })
    }
}

impl<'a> UiPackRecordView<'a> {
    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn iform(&self) -> &'a str {
        self.iform
    }

    pub fn string(&self) -> &'a str {
        self.string
    }

    pub fn perf(&self) -> UiPackPerfView {
        UiPackPerfView {
            uops: self.entry.uops,
            retire_slots: self.entry.retire_slots,
            uops_mite: self.entry.uops_mite,
            uops_ms: self.entry.uops_ms,
            tp: if self.entry.flags & 1 == 1 {
                Some(f64::from_bits(self.entry.tp_bits))
            } else {
                None
            },
            div_cycles: self.entry.div_cycles,
            may_be_eliminated: self.entry.flags & (1 << 1) != 0,
            complex_decoder: self.entry.flags & (1 << 2) != 0,
            n_available_simple_decoders: self.entry.n_available_simple_decoders,
            lcp_stall: self.entry.flags & (1 << 3) != 0,
            implicit_rsp_change: self.entry.implicit_rsp_change,
            can_be_used_by_lsd: self.entry.flags & (1 << 4) != 0,
            cannot_be_in_dsb_due_to_jcc_erratum: self.entry.flags & (1 << 5) != 0,
            no_micro_fusion: self.entry.flags & (1 << 6) != 0,
            no_macro_fusion: self.entry.flags & (1 << 7) != 0,
        }
    }

    pub fn imm_zero(&self) -> bool {
        self.entry.flags & (1 << 8) != 0
    }

    pub fn ports(&self) -> Result<Vec<UiPackPortView<'a>>, UiPackError> {
        let ports_start = usize::try_from(self.entry.ports_start)
            .map_err(|_| UiPackError::InvalidFormat("uipack ports range overflow".to_string()))?;
        let ports_count = usize::try_from(self.entry.ports_count)
            .map_err(|_| UiPackError::InvalidFormat("uipack ports range overflow".to_string()))?;
        let ports_base = usize::try_from(self.view.header.ports_offset).unwrap();
        let mut ports = Vec::with_capacity(ports_count);

        for port_idx in 0..ports_count {
            let start = ports_base + (ports_start + port_idx) * UIPACK_PORT_ENTRY_SIZE;
            let entry = read_port_entry(&self.view.bytes[start..start + UIPACK_PORT_ENTRY_SIZE]);
            ports.push(UiPackPortView {
                key: self.view.resolve_string(entry.key_offset)?,
                count: entry.count,
            });
        }

        Ok(ports)
    }

    pub fn operands(&self) -> Result<Vec<OperandRecord>, UiPackError> {
        Ok(serde_json::from_slice(self.blob(
            self.entry.operands_offset,
            self.entry.operands_size,
            "operands",
        )?)?)
    }

    pub fn latencies(&self) -> Result<Vec<LatencyRecord>, UiPackError> {
        Ok(serde_json::from_slice(self.blob(
            self.entry.latencies_offset,
            self.entry.latencies_size,
            "latencies",
        )?)?)
    }

    pub fn variants(&self) -> Result<BTreeMap<String, PerfVariantRecord>, UiPackError> {
        if self.entry.variants_size == 0 {
            return Ok(BTreeMap::new());
        }
        Ok(serde_json::from_slice(self.blob(
            self.entry.variants_offset,
            self.entry.variants_size,
            "variants",
        )?)?)
    }

    pub fn macro_fusible_with(&self) -> Result<Vec<String>, UiPackError> {
        if self.entry.macro_fusible_size == 0 {
            return Ok(Vec::new());
        }
        Ok(serde_json::from_slice(self.blob(
            self.entry.macro_fusible_offset,
            self.entry.macro_fusible_size,
            "macro_fusible_with",
        )?)?)
    }

    fn blob(&self, offset: u32, size: u32, name: &str) -> Result<&'a [u8], UiPackError> {
        let offset = usize::try_from(offset)
            .map_err(|_| UiPackError::InvalidFormat(format!("uipack {name} offset overflow")))?;
        let size = usize::try_from(size)
            .map_err(|_| UiPackError::InvalidFormat(format!("uipack {name} size overflow")))?;
        let end = offset
            .checked_add(size)
            .ok_or_else(|| UiPackError::InvalidFormat(format!("uipack {name} range overflow")))?;
        if end > self.view.bytes.len() {
            return Err(UiPackError::InvalidFormat(format!(
                "uipack {name} range out of bounds"
            )));
        }
        Ok(&self.view.bytes[offset..end])
    }
}

impl UiPackPerfView {
    pub fn uops(&self) -> i32 {
        self.uops
    }

    pub fn retire_slots(&self) -> i32 {
        self.retire_slots
    }

    pub fn uops_mite(&self) -> i32 {
        self.uops_mite
    }

    pub fn uops_ms(&self) -> i32 {
        self.uops_ms
    }

    pub fn tp(&self) -> Option<f64> {
        self.tp
    }

    pub fn div_cycles(&self) -> u32 {
        self.div_cycles
    }

    pub fn may_be_eliminated(&self) -> bool {
        self.may_be_eliminated
    }

    pub fn complex_decoder(&self) -> bool {
        self.complex_decoder
    }

    pub fn n_available_simple_decoders(&self) -> u32 {
        self.n_available_simple_decoders
    }

    pub fn lcp_stall(&self) -> bool {
        self.lcp_stall
    }

    pub fn implicit_rsp_change(&self) -> i32 {
        self.implicit_rsp_change
    }

    pub fn can_be_used_by_lsd(&self) -> bool {
        self.can_be_used_by_lsd
    }

    pub fn cannot_be_in_dsb_due_to_jcc_erratum(&self) -> bool {
        self.cannot_be_in_dsb_due_to_jcc_erratum
    }

    pub fn no_micro_fusion(&self) -> bool {
        self.no_micro_fusion
    }

    pub fn no_macro_fusion(&self) -> bool {
        self.no_macro_fusion
    }
}

impl<'a> UiPackPortView<'a> {
    pub fn key(&self) -> &'a str {
        self.key
    }

    pub fn count(&self) -> i32 {
        self.count
    }
}

impl UiPackViewIndex {
    pub fn new(view: &UiPackView<'_>) -> Result<Self, UiPackError> {
        let mut by_mnemonic = BTreeMap::new();
        let mut by_iform = BTreeMap::new();

        for index in 0..view.record_count() {
            let record = view.record(index)?;
            by_mnemonic
                .entry(crate::index::normalize_mnemonic(record.string()))
                .or_insert_with(Vec::new)
                .push(index);
            by_iform
                .entry(record.iform().to_ascii_uppercase())
                .or_insert_with(Vec::new)
                .push(index);
        }

        Ok(Self {
            by_mnemonic,
            by_iform,
            empty: Vec::new(),
        })
    }

    pub fn record_indices_for_mnemonic(&self, mnemonic: &str) -> &[u32] {
        self.by_mnemonic
            .get(&crate::index::normalize_mnemonic(mnemonic))
            .map(Vec::as_slice)
            .unwrap_or(self.empty.as_slice())
    }

    pub fn record_indices_for_iform(&self, iform: &str) -> &[u32] {
        self.by_iform
            .get(&iform.to_ascii_uppercase())
            .map(Vec::as_slice)
            .unwrap_or(self.empty.as_slice())
    }
}

pub fn encode_uipack(pack: &DataPack, arch: &str) -> Result<Vec<u8>, UiPackError> {
    for record in &pack.instructions {
        if record.arch != arch {
            return Err(UiPackError::InvalidFormat(format!(
                "uipack requires single arch pack '{arch}', found '{}'",
                record.arch
            )));
        }
    }

    let mut strings = StringTable::new();
    let schema_offset = strings.intern(&pack.schema_version)?;
    let arch_offset = strings.intern(arch)?;

    let mut raw_records = Vec::with_capacity(pack.instructions.len());
    let mut port_entries = Vec::new();

    for record in &pack.instructions {
        let iform_offset = strings.intern(&record.iform)?;
        let string_offset = strings.intern(&record.string)?;
        let ports_start = u32::try_from(port_entries.len()).map_err(|_| {
            UiPackError::InvalidFormat("too many port entries for uipack".to_string())
        })?;

        for (port, count) in &record.perf.ports {
            port_entries.push(PortEntry {
                key_offset: strings.intern(port)?,
                count: *count,
            });
        }

        let ports_count = u32::try_from(record.perf.ports.len()).map_err(|_| {
            UiPackError::InvalidFormat("too many ports on instruction record".to_string())
        })?;
        let operands = serde_json::to_vec(&record.perf.operands)?;
        let latencies = serde_json::to_vec(&record.perf.latencies)?;
        let variants = serde_json::to_vec(&record.perf.variants)?;
        let macro_fusible_with = serde_json::to_vec(&record.perf.macro_fusible_with)?;

        raw_records.push(RawRecordEntry {
            iform_offset,
            string_offset,
            ports_start,
            ports_count,
            uops: record.perf.uops,
            retire_slots: record.perf.retire_slots,
            uops_mite: record.perf.uops_mite,
            uops_ms: record.perf.uops_ms,
            tp_bits: record.perf.tp.unwrap_or_default().to_bits(),
            flags: u32::from(record.perf.tp.is_some())
                | (u32::from(record.perf.may_be_eliminated) << 1)
                | (u32::from(record.perf.complex_decoder) << 2)
                | (u32::from(record.perf.lcp_stall) << 3)
                | (u32::from(record.perf.can_be_used_by_lsd) << 4)
                | (u32::from(record.perf.cannot_be_in_dsb_due_to_jcc_erratum) << 5)
                | (u32::from(record.perf.no_micro_fusion) << 6)
                | (u32::from(record.perf.no_macro_fusion) << 7)
                | (u32::from(record.imm_zero) << 8),
            div_cycles: record.perf.div_cycles,
            n_available_simple_decoders: record.perf.n_available_simple_decoders,
            implicit_rsp_change: record.perf.implicit_rsp_change,
            operands,
            latencies,
            variants,
            macro_fusible_with,
        });
    }

    let strings_bytes = strings.into_bytes();
    let strings_offset = u32::try_from(UIPACK_HEADER_SIZE)
        .map_err(|_| UiPackError::InvalidFormat("uipack header too large".to_string()))?;
    let records_offset = u32::try_from(align_up(
        usize::try_from(strings_offset).unwrap() + strings_bytes.len(),
        UIPACK_ALIGNMENT,
    ))
    .map_err(|_| UiPackError::InvalidFormat("uipack too large".to_string()))?;
    let ports_offset = u32::try_from(align_up(
        usize::try_from(records_offset).unwrap() + raw_records.len() * UIPACK_RECORD_SIZE,
        UIPACK_ALIGNMENT,
    ))
    .map_err(|_| UiPackError::InvalidFormat("uipack too large".to_string()))?;
    let blobs_offset = align_up(
        usize::try_from(ports_offset).unwrap() + port_entries.len() * UIPACK_PORT_ENTRY_SIZE,
        UIPACK_ALIGNMENT,
    );
    let mut record_entries = Vec::with_capacity(raw_records.len());
    let mut blobs = Vec::new();
    for raw in raw_records {
        let operands_offset = u32::try_from(blobs_offset + blobs.len())
            .map_err(|_| UiPackError::InvalidFormat("uipack too large".to_string()))?;
        let operands_size = u32::try_from(raw.operands.len())
            .map_err(|_| UiPackError::InvalidFormat("uipack operands too large".to_string()))?;
        blobs.extend_from_slice(&raw.operands);

        let latencies_offset = u32::try_from(blobs_offset + blobs.len())
            .map_err(|_| UiPackError::InvalidFormat("uipack too large".to_string()))?;
        let latencies_size = u32::try_from(raw.latencies.len())
            .map_err(|_| UiPackError::InvalidFormat("uipack latencies too large".to_string()))?;
        blobs.extend_from_slice(&raw.latencies);

        let variants_offset = u32::try_from(blobs_offset + blobs.len())
            .map_err(|_| UiPackError::InvalidFormat("uipack too large".to_string()))?;
        let variants_size = u32::try_from(raw.variants.len())
            .map_err(|_| UiPackError::InvalidFormat("uipack variants too large".to_string()))?;
        blobs.extend_from_slice(&raw.variants);

        let macro_fusible_offset = u32::try_from(blobs_offset + blobs.len())
            .map_err(|_| UiPackError::InvalidFormat("uipack too large".to_string()))?;
        let macro_fusible_size = u32::try_from(raw.macro_fusible_with.len()).map_err(|_| {
            UiPackError::InvalidFormat("uipack macro_fusible_with too large".to_string())
        })?;
        blobs.extend_from_slice(&raw.macro_fusible_with);

        record_entries.push(RecordEntry {
            iform_offset: raw.iform_offset,
            string_offset: raw.string_offset,
            ports_start: raw.ports_start,
            ports_count: raw.ports_count,
            uops: raw.uops,
            retire_slots: raw.retire_slots,
            uops_mite: raw.uops_mite,
            uops_ms: raw.uops_ms,
            tp_bits: raw.tp_bits,
            flags: raw.flags,
            operands_offset,
            operands_size,
            latencies_offset,
            latencies_size,
            div_cycles: raw.div_cycles,
            n_available_simple_decoders: raw.n_available_simple_decoders,
            implicit_rsp_change: raw.implicit_rsp_change,
            variants_offset,
            variants_size,
            macro_fusible_offset,
            macro_fusible_size,
        });
    }
    let file_len = u64::try_from(blobs_offset + blobs.len())
        .map_err(|_| UiPackError::InvalidFormat("uipack too large".to_string()))?;

    let strings_size = u32::try_from(strings_bytes.len())
        .map_err(|_| UiPackError::InvalidFormat("string section too large".to_string()))?;
    let records_count = u32::try_from(record_entries.len())
        .map_err(|_| UiPackError::InvalidFormat("too many instruction records".to_string()))?;
    let ports_count = u32::try_from(port_entries.len())
        .map_err(|_| UiPackError::InvalidFormat("too many port entries".to_string()))?;

    let header = UiPackHeader {
        version: UIPACK_VERSION,
        header_size: UIPACK_HEADER_SIZE as u16,
        checksum_kind: UIPACK_CHECKSUM_FNV1A64,
        file_len,
        checksum: 0,
        arch_offset,
        strings_offset,
        strings_size,
        records_offset,
        records_count,
        ports_offset,
        ports_count,
        schema_offset,
    };

    let mut bytes = vec![0_u8; usize::try_from(file_len).unwrap()];
    write_header(&mut bytes[..UIPACK_HEADER_SIZE], &header);
    bytes[usize::try_from(strings_offset).unwrap()
        ..usize::try_from(strings_offset).unwrap() + strings_bytes.len()]
        .copy_from_slice(&strings_bytes);

    let records_base = usize::try_from(records_offset).unwrap();
    for (idx, record) in record_entries.iter().enumerate() {
        let start = records_base + idx * UIPACK_RECORD_SIZE;
        write_record_entry(&mut bytes[start..start + UIPACK_RECORD_SIZE], record);
    }

    let ports_base = usize::try_from(ports_offset).unwrap();
    for (idx, port) in port_entries.iter().enumerate() {
        let start = ports_base + idx * UIPACK_PORT_ENTRY_SIZE;
        write_port_entry(&mut bytes[start..start + UIPACK_PORT_ENTRY_SIZE], port);
    }
    bytes[blobs_offset..blobs_offset + blobs.len()].copy_from_slice(&blobs);

    let checksum = fnv1a64_with_zeroed_checksum(&bytes);
    bytes[CHECKSUM_OFFSET..CHECKSUM_OFFSET + 8].copy_from_slice(&checksum.to_le_bytes());
    Ok(bytes)
}

pub fn read_uipack_header(bytes: &[u8]) -> Result<UiPackHeader, UiPackError> {
    if bytes.len() < UIPACK_HEADER_SIZE {
        return Err(UiPackError::InvalidFormat(
            "uipack header truncated".to_string(),
        ));
    }
    if bytes[..8] != UIPACK_MAGIC {
        return Err(UiPackError::InvalidFormat(
            "invalid uipack magic".to_string(),
        ));
    }

    let header = UiPackHeader {
        version: read_u16(bytes, 8),
        header_size: read_u16(bytes, 10),
        checksum_kind: read_u16(bytes, 12),
        file_len: read_u64(bytes, 16),
        checksum: read_u64(bytes, 24),
        arch_offset: read_u32(bytes, 32),
        strings_offset: read_u32(bytes, 36),
        strings_size: read_u32(bytes, 40),
        records_offset: read_u32(bytes, 44),
        records_count: read_u32(bytes, 48),
        ports_offset: read_u32(bytes, 52),
        ports_count: read_u32(bytes, 56),
        schema_offset: read_u32(bytes, 60),
    };

    if header.version != UIPACK_VERSION {
        return Err(UiPackError::InvalidFormat(format!(
            "unsupported uipack version {} (expected {})",
            header.version, UIPACK_VERSION
        )));
    }
    if header.header_size as usize != UIPACK_HEADER_SIZE {
        return Err(UiPackError::InvalidFormat(format!(
            "unsupported uipack header size {}",
            header.header_size
        )));
    }
    if header.checksum_kind != UIPACK_CHECKSUM_FNV1A64 {
        return Err(UiPackError::InvalidFormat(format!(
            "unsupported uipack checksum kind {}",
            header.checksum_kind
        )));
    }
    if header.file_len != bytes.len() as u64 {
        return Err(UiPackError::InvalidFormat(format!(
            "uipack file length mismatch: header {} != actual {}",
            header.file_len,
            bytes.len()
        )));
    }

    validate_section(
        "strings",
        header.strings_offset,
        header.strings_size,
        1,
        bytes.len(),
        header.header_size as usize,
    )?;
    validate_section(
        "records",
        header.records_offset,
        header.records_count,
        record_size_for_version(header.version)?,
        bytes.len(),
        header.header_size as usize,
    )?;
    validate_section(
        "ports",
        header.ports_offset,
        header.ports_count,
        UIPACK_PORT_ENTRY_SIZE,
        bytes.len(),
        header.header_size as usize,
    )?;

    if usize::try_from(header.records_offset).unwrap() % UIPACK_ALIGNMENT != 0 {
        return Err(UiPackError::InvalidFormat(
            "uipack records section misaligned".to_string(),
        ));
    }
    if usize::try_from(header.ports_offset).unwrap() % UIPACK_ALIGNMENT != 0 {
        return Err(UiPackError::InvalidFormat(
            "uipack ports section misaligned".to_string(),
        ));
    }
    if header.arch_offset >= header.strings_size {
        return Err(UiPackError::InvalidFormat(
            "uipack arch string offset out of bounds".to_string(),
        ));
    }
    if header.schema_offset >= header.strings_size {
        return Err(UiPackError::InvalidFormat(
            "uipack schema string offset out of bounds".to_string(),
        ));
    }

    let actual_checksum = fnv1a64_with_zeroed_checksum(bytes);
    if actual_checksum != header.checksum {
        return Err(UiPackError::InvalidFormat(format!(
            "uipack checksum mismatch: expected {:016x}, got {:016x}",
            header.checksum, actual_checksum
        )));
    }

    Ok(header)
}

pub fn load_uipack_bytes(bytes: &[u8]) -> Result<DataPack, UiPackError> {
    let view = UiPackView::from_bytes(bytes)?;
    view.to_data_pack()
}

pub fn load_pack_bytes(bytes: &[u8]) -> Result<DataPack, UiPackError> {
    load_uipack_bytes(bytes)
}

#[derive(Clone, Debug)]
struct RawRecordEntry {
    iform_offset: u32,
    string_offset: u32,
    ports_start: u32,
    ports_count: u32,
    uops: i32,
    retire_slots: i32,
    uops_mite: i32,
    uops_ms: i32,
    tp_bits: u64,
    flags: u32,
    div_cycles: u32,
    n_available_simple_decoders: u32,
    implicit_rsp_change: i32,
    operands: Vec<u8>,
    latencies: Vec<u8>,
    variants: Vec<u8>,
    macro_fusible_with: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
struct RecordEntry {
    iform_offset: u32,
    string_offset: u32,
    ports_start: u32,
    ports_count: u32,
    uops: i32,
    retire_slots: i32,
    uops_mite: i32,
    uops_ms: i32,
    tp_bits: u64,
    flags: u32,
    operands_offset: u32,
    operands_size: u32,
    latencies_offset: u32,
    latencies_size: u32,
    div_cycles: u32,
    n_available_simple_decoders: u32,
    implicit_rsp_change: i32,
    variants_offset: u32,
    variants_size: u32,
    macro_fusible_offset: u32,
    macro_fusible_size: u32,
}

#[derive(Clone, Copy, Debug)]
struct PortEntry {
    key_offset: u32,
    count: i32,
}

struct StringTable {
    offsets: BTreeMap<String, u32>,
    bytes: Vec<u8>,
}

impl StringTable {
    fn new() -> Self {
        Self {
            offsets: BTreeMap::new(),
            bytes: Vec::new(),
        }
    }

    fn intern(&mut self, value: &str) -> Result<u32, UiPackError> {
        if let Some(offset) = self.offsets.get(value) {
            return Ok(*offset);
        }

        let offset = u32::try_from(self.bytes.len())
            .map_err(|_| UiPackError::InvalidFormat("string table too large".to_string()))?;
        let len = u32::try_from(value.len())
            .map_err(|_| UiPackError::InvalidFormat("string too large for uipack".to_string()))?;
        self.bytes.extend_from_slice(&len.to_le_bytes());
        self.bytes.extend_from_slice(value.as_bytes());
        self.offsets.insert(value.to_string(), offset);
        Ok(offset)
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

fn validate_section(
    name: &str,
    offset: u32,
    count_or_size: u32,
    stride: usize,
    file_len: usize,
    header_size: usize,
) -> Result<(), UiPackError> {
    let offset = usize::try_from(offset)
        .map_err(|_| UiPackError::InvalidFormat(format!("uipack {name} offset overflow")))?;
    if offset < header_size || offset > file_len {
        return Err(UiPackError::InvalidFormat(format!(
            "uipack {name} section out of bounds"
        )));
    }

    let len = usize::try_from(count_or_size)
        .map_err(|_| UiPackError::InvalidFormat(format!("uipack {name} length overflow")))?
        .checked_mul(stride)
        .ok_or_else(|| UiPackError::InvalidFormat(format!("uipack {name} length overflow")))?;
    let end = offset.checked_add(len).ok_or_else(|| {
        UiPackError::InvalidFormat(format!("uipack {name} section length overflow"))
    })?;
    if end > file_len {
        return Err(UiPackError::InvalidFormat(format!(
            "uipack {name} section out of bounds"
        )));
    }
    Ok(())
}

fn validate_record_ports_range(entry: RecordEntry, ports_count: u32) -> Result<(), UiPackError> {
    let ports_start = usize::try_from(entry.ports_start)
        .map_err(|_| UiPackError::InvalidFormat("uipack ports range overflow".to_string()))?;
    let ports_len = usize::try_from(entry.ports_count)
        .map_err(|_| UiPackError::InvalidFormat("uipack ports range overflow".to_string()))?;
    let ports_end = ports_start
        .checked_add(ports_len)
        .ok_or_else(|| UiPackError::InvalidFormat("uipack ports range overflow".to_string()))?;
    if ports_end > usize::try_from(ports_count).unwrap() {
        return Err(UiPackError::InvalidFormat(
            "uipack record ports range out of bounds".to_string(),
        ));
    }

    Ok(())
}

fn read_string_ref(strings: &[u8], offset: u32) -> Result<&str, UiPackError> {
    let offset = usize::try_from(offset)
        .map_err(|_| UiPackError::InvalidFormat("uipack string offset overflow".to_string()))?;
    let len_field_end = offset
        .checked_add(4)
        .ok_or_else(|| UiPackError::InvalidFormat("uipack string offset overflow".to_string()))?;
    if len_field_end > strings.len() {
        return Err(UiPackError::InvalidFormat(
            "uipack string offset out of bounds".to_string(),
        ));
    }
    let len = usize::try_from(read_u32(strings, offset))
        .map_err(|_| UiPackError::InvalidFormat("uipack string length overflow".to_string()))?;
    let start = offset + 4;
    let end = start
        .checked_add(len)
        .ok_or_else(|| UiPackError::InvalidFormat("uipack string length overflow".to_string()))?;
    if end > strings.len() {
        return Err(UiPackError::InvalidFormat(
            "uipack string extends past section".to_string(),
        ));
    }

    std::str::from_utf8(&strings[start..end])
        .map_err(|err| UiPackError::InvalidFormat(format!("uipack string is not utf-8: {err}")))
}

fn write_header(dst: &mut [u8], header: &UiPackHeader) {
    dst[..8].copy_from_slice(&UIPACK_MAGIC);
    dst[8..10].copy_from_slice(&header.version.to_le_bytes());
    dst[10..12].copy_from_slice(&header.header_size.to_le_bytes());
    dst[12..14].copy_from_slice(&header.checksum_kind.to_le_bytes());
    dst[14..16].copy_from_slice(&0_u16.to_le_bytes());
    dst[16..24].copy_from_slice(&header.file_len.to_le_bytes());
    dst[24..32].copy_from_slice(&header.checksum.to_le_bytes());
    dst[32..36].copy_from_slice(&header.arch_offset.to_le_bytes());
    dst[36..40].copy_from_slice(&header.strings_offset.to_le_bytes());
    dst[40..44].copy_from_slice(&header.strings_size.to_le_bytes());
    dst[44..48].copy_from_slice(&header.records_offset.to_le_bytes());
    dst[48..52].copy_from_slice(&header.records_count.to_le_bytes());
    dst[52..56].copy_from_slice(&header.ports_offset.to_le_bytes());
    dst[56..60].copy_from_slice(&header.ports_count.to_le_bytes());
    dst[60..64].copy_from_slice(&header.schema_offset.to_le_bytes());
}

fn record_size_for_version(version: u16) -> Result<usize, UiPackError> {
    if version == UIPACK_VERSION {
        Ok(UIPACK_RECORD_SIZE)
    } else {
        Err(UiPackError::InvalidFormat(format!(
            "unsupported uipack version {version} (expected {UIPACK_VERSION})"
        )))
    }
}

fn write_record_entry(dst: &mut [u8], record: &RecordEntry) {
    dst[0..4].copy_from_slice(&record.iform_offset.to_le_bytes());
    dst[4..8].copy_from_slice(&record.string_offset.to_le_bytes());
    dst[8..12].copy_from_slice(&record.ports_start.to_le_bytes());
    dst[12..16].copy_from_slice(&record.ports_count.to_le_bytes());
    dst[16..20].copy_from_slice(&record.uops.to_le_bytes());
    dst[20..24].copy_from_slice(&record.retire_slots.to_le_bytes());
    dst[24..28].copy_from_slice(&record.uops_mite.to_le_bytes());
    dst[28..32].copy_from_slice(&record.uops_ms.to_le_bytes());
    dst[32..40].copy_from_slice(&record.tp_bits.to_le_bytes());
    dst[40..44].copy_from_slice(&record.flags.to_le_bytes());
    dst[44..48].copy_from_slice(&record.operands_offset.to_le_bytes());
    dst[48..52].copy_from_slice(&record.operands_size.to_le_bytes());
    dst[52..56].copy_from_slice(&record.latencies_offset.to_le_bytes());
    dst[56..60].copy_from_slice(&record.latencies_size.to_le_bytes());
    dst[60..64].copy_from_slice(&record.div_cycles.to_le_bytes());
    dst[64..68].copy_from_slice(&record.n_available_simple_decoders.to_le_bytes());
    dst[68..72].copy_from_slice(&record.implicit_rsp_change.to_le_bytes());
    dst[72..76].copy_from_slice(&record.variants_offset.to_le_bytes());
    dst[76..80].copy_from_slice(&record.variants_size.to_le_bytes());
    dst[80..84].copy_from_slice(&record.macro_fusible_offset.to_le_bytes());
    dst[84..88].copy_from_slice(&record.macro_fusible_size.to_le_bytes());
}

fn write_port_entry(dst: &mut [u8], port: &PortEntry) {
    dst[0..4].copy_from_slice(&port.key_offset.to_le_bytes());
    dst[4..8].copy_from_slice(&port.count.to_le_bytes());
}

fn read_record_entry(src: &[u8], version: u16) -> RecordEntry {
    RecordEntry {
        iform_offset: read_u32(src, 0),
        string_offset: read_u32(src, 4),
        ports_start: read_u32(src, 8),
        ports_count: read_u32(src, 12),
        uops: read_i32(src, 16),
        retire_slots: read_i32(src, 20),
        uops_mite: read_i32(src, 24),
        uops_ms: read_i32(src, 28),
        tp_bits: read_u64(src, 32),
        flags: read_u32(src, 40),
        operands_offset: read_u32(src, 44),
        operands_size: read_u32(src, 48),
        latencies_offset: read_u32(src, 52),
        latencies_size: read_u32(src, 56),
        div_cycles: read_u32(src, 60),
        n_available_simple_decoders: read_u32(src, 64),
        implicit_rsp_change: read_i32(src, 68),
        variants_offset: if version >= 4 { read_u32(src, 72) } else { 0 },
        variants_size: if version >= 4 { read_u32(src, 76) } else { 0 },
        macro_fusible_offset: if version >= 5 { read_u32(src, 80) } else { 0 },
        macro_fusible_size: if version >= 5 { read_u32(src, 84) } else { 0 },
    }
}

fn read_port_entry(src: &[u8]) -> PortEntry {
    PortEntry {
        key_offset: read_u32(src, 0),
        count: read_i32(src, 4),
    }
}

fn read_u16(src: &[u8], offset: usize) -> u16 {
    let mut bytes = [0_u8; 2];
    bytes.copy_from_slice(&src[offset..offset + 2]);
    u16::from_le_bytes(bytes)
}

fn read_u32(src: &[u8], offset: usize) -> u32 {
    let mut bytes = [0_u8; 4];
    bytes.copy_from_slice(&src[offset..offset + 4]);
    u32::from_le_bytes(bytes)
}

fn read_i32(src: &[u8], offset: usize) -> i32 {
    let mut bytes = [0_u8; 4];
    bytes.copy_from_slice(&src[offset..offset + 4]);
    i32::from_le_bytes(bytes)
}

fn read_u64(src: &[u8], offset: usize) -> u64 {
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&src[offset..offset + 8]);
    u64::from_le_bytes(bytes)
}

fn fnv1a64_with_zeroed_checksum(bytes: &[u8]) -> u64 {
    let mut hash = FNV1A64_OFFSET_BASIS;

    for (idx, byte) in bytes.iter().enumerate() {
        let value = if (CHECKSUM_OFFSET..CHECKSUM_OFFSET + 8).contains(&idx) {
            0
        } else {
            *byte
        };
        hash ^= u64::from(value);
        hash = hash.wrapping_mul(FNV1A64_PRIME);
    }

    hash
}

fn align_up(value: usize, align: usize) -> usize {
    if value.is_multiple_of(align) {
        value
    } else {
        value + (align - value % align)
    }
}
