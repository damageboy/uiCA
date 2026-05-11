use std::collections::BTreeMap;

use uica_data::InstructionRecord;

pub trait InstrRecordLike {
    fn iform(&self) -> &str;
    fn string(&self) -> &str;
    fn xml_attrs(&self) -> &BTreeMap<String, String>;
    fn imm_zero(&self) -> bool;
}

impl InstrRecordLike for InstructionRecord {
    fn iform(&self) -> &str {
        &self.iform
    }
    fn string(&self) -> &str {
        &self.string
    }
    fn xml_attrs(&self) -> &BTreeMap<String, String> {
        &self.xml_attrs
    }
    fn imm_zero(&self) -> bool {
        self.imm_zero
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NormalizedInstr {
    pub mnemonic: String,
    /// Exact XED iform decoded for this instruction. Python indexes rows by this key first.
    pub decoded_iform: String,
    /// Operand-kind signature built from decoder/XED-derived operand kinds (e.g. `GPRv_GPRv`).
    /// Empty string when not known; matcher falls back to mnemonic-only.
    pub iform_signature: String,
    /// Maximum operand register size in bytes (e.g. 8=R64, 4=R32, 2=R16).
    /// Zero when not known. Used to disambiguate records whose iforms share
    /// the same signature (e.g. MOV_GPRv_GPRv_89 has R16/R32/R64 variants).
    pub max_op_size_bytes: u8,
    /// Immediate value, used like Python's XML attr predicates (`immzero`) to
    /// distinguish zero-immediate records from general immediate records.
    pub immediate: Option<i64>,
    /// True when an explicit operand uses AH/BH/CH/DH; mirrors Python/XED
    /// R8h vs R8l attribute matching for uops.info records.
    pub uses_high8_reg: bool,
    /// Explicit register operands in instruction operand order for R8h/R8l matching.
    pub explicit_reg_operands: Vec<String>,
    /// XED/XML match attributes used by Python `xed.matchXMLAttributes()`.
    pub xml_attrs: BTreeMap<String, String>,
    /// XED `agen` attribute for LEA addressing forms (e.g. B_IS_D8).
    pub agen: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct NormalizedInstrRef<'a> {
    pub mnemonic: &'a str,
    pub decoded_iform: &'a str,
    pub iform_signature: &'a str,
    pub max_op_size_bytes: u8,
    pub immediate: Option<i64>,
    pub uses_high8_reg: bool,
    pub explicit_reg_operands: &'a [String],
    pub xml_attrs: &'a BTreeMap<String, String>,
    pub agen: Option<&'a str>,
}

impl NormalizedInstr {
    pub fn as_ref(&self) -> NormalizedInstrRef<'_> {
        NormalizedInstrRef {
            mnemonic: &self.mnemonic,
            decoded_iform: &self.decoded_iform,
            iform_signature: &self.iform_signature,
            max_op_size_bytes: self.max_op_size_bytes,
            immediate: self.immediate,
            uses_high8_reg: self.uses_high8_reg,
            explicit_reg_operands: &self.explicit_reg_operands,
            xml_attrs: &self.xml_attrs,
            agen: self.agen.as_deref(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateRecord {
    pub iform: String,
    pub string: String,
}

pub fn match_instruction<'a>(
    instruction: &NormalizedInstr,
    candidates: &'a [CandidateRecord],
) -> Option<&'a CandidateRecord> {
    let normalized_mnemonic = normalize_mnemonic(&instruction.mnemonic);

    candidates
        .iter()
        .find(|candidate| normalize_mnemonic(&candidate.string) == normalized_mnemonic)
        .or_else(|| {
            candidates
                .iter()
                .find(|candidate| normalize_iform_prefix(&candidate.iform) == normalized_mnemonic)
        })
}

pub fn match_instruction_record<'a>(
    instruction: &NormalizedInstr,
    candidates: &'a [InstructionRecord],
) -> Option<&'a InstructionRecord> {
    match_instruction_record_ref(instruction.as_ref(), candidates)
}

pub fn match_instruction_record_ref<'a>(
    instruction: NormalizedInstrRef<'_>,
    candidates: &'a [InstructionRecord],
) -> Option<&'a InstructionRecord> {
    match_instruction_record_iter(instruction, candidates.iter())
}

pub fn match_instruction_record_iter<'a, R, I>(
    instruction: NormalizedInstrRef<'_>,
    candidates: I,
) -> Option<&'a R>
where
    R: InstrRecordLike + 'a,
    I: IntoIterator<Item = &'a R>,
{
    let candidates: Vec<&'a R> = candidates.into_iter().collect();
    let normalized_mnemonic = normalize_mnemonic(instruction.mnemonic);
    let raw_mnemonic = raw_mnemonic(instruction.mnemonic);
    let sig = instruction.iform_signature.trim();

    let max_size = instruction.max_op_size_bytes;
    let decoded_iform = instruction.decoded_iform.trim();
    if !decoded_iform.is_empty() {
        let exact_matches: Vec<&R> = candidates
            .iter()
            .copied()
            .filter(|candidate| candidate.iform() == decoded_iform)
            .collect();
        let exact_matches = filter_xml_attr_matches(exact_matches, instruction.xml_attrs);
        if exact_matches.is_empty() {
            return None;
        }
        return best_record_match(
            exact_matches,
            max_size,
            instruction.immediate,
            instruction.uses_high8_reg,
            instruction.explicit_reg_operands,
            instruction.agen,
        );
    }

    // Prefer candidates whose iform carries the same operand-kind signature,
    // e.g. ADC_GPRv_GPRv_11 for a register/register ADC.
    if !sig.is_empty() {
        let sig_matches: Vec<&R> = candidates
            .iter()
            .copied()
            .filter(|candidate| {
                iform_matches_signature(candidate.iform(), &normalized_mnemonic, sig)
                    || (raw_mnemonic != normalized_mnemonic
                        && iform_matches_signature(candidate.iform(), &raw_mnemonic, sig))
            })
            .collect();
        let sig_matches = filter_xml_attr_matches(sig_matches, instruction.xml_attrs);
        if !sig_matches.is_empty() {
            return best_record_match(
                sig_matches,
                max_size,
                instruction.immediate,
                instruction.uses_high8_reg,
                instruction.explicit_reg_operands,
                instruction.agen,
            );
        }
        // Python parity: `getInstructions()` indexes uops.info data by exact
        // XED iform first (`archData.instrData.get(instrD['iform'], [])`). If
        // the decoded iform signature has no matching record (e.g. XED `VGPR`
        // vs instruction-record `GPR` MULX), Python creates `UnknownInstr` instead of
        // falling back to a mnemonic-only record.
        return None;
    }

    // Fallback: string mnemonic match when no iform signature is available.
    let string_matches: Vec<&R> = candidates
        .iter()
        .copied()
        .filter(|c| normalize_mnemonic(c.string()) == normalized_mnemonic)
        .collect();
    let string_matches = filter_xml_attr_matches(string_matches, instruction.xml_attrs);
    if !string_matches.is_empty() {
        return best_record_match(
            string_matches,
            max_size,
            instruction.immediate,
            instruction.uses_high8_reg,
            instruction.explicit_reg_operands,
            instruction.agen,
        );
    }
    let iform_matches: Vec<&R> = candidates
        .iter()
        .copied()
        .filter(|candidate| normalize_iform_prefix(candidate.iform()) == normalized_mnemonic)
        .collect();
    let iform_matches = filter_xml_attr_matches(iform_matches, instruction.xml_attrs);
    best_record_match(
        iform_matches,
        max_size,
        instruction.immediate,
        instruction.uses_high8_reg,
        instruction.explicit_reg_operands,
        instruction.agen,
    )
}

fn filter_xml_attr_matches<'a, R>(
    candidates: Vec<&'a R>,
    decoded_attrs: &BTreeMap<String, String>,
) -> Vec<&'a R>
where
    R: InstrRecordLike + 'a,
{
    candidates
        .into_iter()
        .filter(|record| xml_attrs_match(decoded_attrs, record.xml_attrs()))
        .collect()
}

fn xml_attrs_match(
    decoded_attrs: &BTreeMap<String, String>,
    record_attrs: &BTreeMap<String, String>,
) -> bool {
    for (key, record_value) in record_attrs {
        let Some(decoded_value) = decoded_attrs.get(key) else {
            continue;
        };
        if key == "rm" {
            if !record_value.contains(decoded_value) {
                return false;
            }
        } else if decoded_value != record_value {
            return false;
        }
    }
    true
}

fn best_record_match<'a, R>(
    candidates: Vec<&'a R>,
    max_size: u8,
    immediate: Option<i64>,
    uses_high8_reg: bool,
    explicit_reg_operands: &[String],
    agen: Option<&str>,
) -> Option<&'a R>
where
    R: InstrRecordLike + 'a,
{
    let size_tag = match max_size {
        8 => "R64",
        4 => "R32",
        2 => "R16",
        1 => "R8",
        _ => "",
    };
    let sized: Vec<&R> = if size_tag.is_empty() {
        candidates
    } else {
        let filtered: Vec<&R> = candidates
            .iter()
            .copied()
            .filter(|c| c.string().contains(size_tag))
            .collect();
        if filtered.is_empty() {
            candidates
        } else {
            filtered
        }
    };

    let sized: Vec<&R> = if let Some(agen) = agen {
        let lea_prefix = format!("LEA_{agen} ");
        let filtered: Vec<&R> = sized
            .iter()
            .copied()
            .filter(|c| c.string().starts_with(&lea_prefix))
            .collect();
        if filtered.is_empty() {
            sized
        } else {
            filtered
        }
    } else {
        sized
    };

    let sized: Vec<&R> = prefer_explicit_mask_form(sized, explicit_reg_operands);

    let sized: Vec<&R> = if sized
        .iter()
        .any(|c| c.string().contains("R8h") || c.string().contains("R8l"))
    {
        let explicit_tags = explicit_r8_tags(explicit_reg_operands);
        let filtered: Vec<&R> = if explicit_tags.is_empty() {
            sized
                .iter()
                .copied()
                .filter(|c| {
                    if uses_high8_reg {
                        c.string().contains("R8h")
                    } else {
                        c.string().contains("R8l") && !c.string().contains("R8h")
                    }
                })
                .collect()
        } else {
            sized
                .iter()
                .copied()
                .filter(|c| record_r8_tags(c.string()) == explicit_tags)
                .collect()
        };
        if filtered.is_empty() {
            sized
        } else {
            filtered
        }
    } else {
        sized
    };

    if let Some(imm) = immediate {
        if imm == 0 {
            if let Some(hit) = sized.iter().find(|c| c.imm_zero()) {
                return Some(*hit);
            }
        } else if let Some(hit) = sized.iter().find(|c| !c.imm_zero()) {
            return Some(*hit);
        }
    }

    sized.into_iter().next()
}

fn prefer_explicit_mask_form<'a, R>(
    candidates: Vec<&'a R>,
    explicit_reg_operands: &[String],
) -> Vec<&'a R>
where
    R: InstrRecordLike + 'a,
{
    let has_mask_alternatives = candidates.iter().any(|c| has_evex_mask_operand(c.string()))
        && candidates
            .iter()
            .any(|c| !has_evex_mask_operand(c.string()));
    if !has_mask_alternatives {
        return candidates;
    }

    let has_explicit_mask = explicit_reg_operands
        .iter()
        .any(|reg| reg.to_ascii_uppercase().starts_with('K'));
    let filtered: Vec<&R> = candidates
        .iter()
        .copied()
        .filter(|c| has_evex_mask_operand(c.string()) == has_explicit_mask)
        .collect();
    if filtered.is_empty() {
        candidates
    } else {
        filtered
    }
}

fn has_evex_mask_operand(string: &str) -> bool {
    // Python parity: XED reports implicit K0 for unmasked EVEX instructions,
    // but `instructions.py` drops K0 unless asm explicitly contains `k0`.
    // uops.info marks the opmask operand as a bare `K` between operands.
    string.contains(", K,")
}

fn explicit_r8_tags(regs: &[String]) -> Vec<&'static str> {
    regs.iter()
        .filter(|reg| crate::x64::get_reg_size(reg) == 8)
        .map(|reg| {
            if crate::x64::is_high8_reg(reg) {
                "R8h"
            } else {
                "R8l"
            }
        })
        .collect()
}

fn record_r8_tags(string: &str) -> Vec<&'static str> {
    let Some(operands) = string
        .split_once('(')
        .and_then(|(_, rest)| rest.strip_suffix(')'))
    else {
        return Vec::new();
    };
    operands
        .split(',')
        .filter_map(|operand| {
            let operand = operand.trim();
            if operand.contains("R8h") {
                Some("R8h")
            } else if operand.contains("R8l") {
                Some("R8l")
            } else {
                None
            }
        })
        .collect()
}

/// True if the record's iform starts with `<mnemonic>_<signature>` (with an
/// optional `_<suffix>` allowed — uops.info adds disambiguators like `_11`).
/// Comparison is case-insensitive because uops.info iforms retain the
/// lowercase `v`/`z`/`w`/`b` size tags from Intel SDM (e.g. `ADC_GPRv_GPRv`).
fn iform_matches_signature(iform: &str, mnemonic: &str, signature: &str) -> bool {
    let normalized_signature = if mnemonic == "LEA" {
        signature.replace("MEM", "AGEN")
    } else {
        signature.to_string()
    };
    let expected_prefix = format!("{}_{}", mnemonic, normalized_signature);
    if iform.eq_ignore_ascii_case(&expected_prefix) {
        return true;
    }
    let iform_lower = iform.to_ascii_lowercase();
    let prefix_lower = format!("{}_", expected_prefix.to_ascii_lowercase());
    if let Some(rest) = iform_lower.strip_prefix(&prefix_lower) {
        return rest.chars().all(|c| c.is_ascii_alphanumeric());
    }
    false
}

pub fn normalize_mnemonic(text: &str) -> String {
    canonical_mnemonic_alias(raw_mnemonic(text).as_str()).to_string()
}

fn raw_mnemonic(text: &str) -> String {
    text.trim()
        .split(|ch: char| ch.is_whitespace() || ch == '(')
        .find(|part| !part.is_empty())
        .unwrap_or("")
        .to_ascii_uppercase()
}

fn canonical_mnemonic_alias(mnemonic: &str) -> &str {
    if mnemonic.starts_with("VCMP") {
        if mnemonic.ends_with("PS") {
            return "VCMPPS";
        }
        if mnemonic.ends_with("PD") {
            return "VCMPPD";
        }
        if mnemonic.ends_with("SS") {
            return "VCMPSS";
        }
        if mnemonic.ends_with("SD") {
            return "VCMPSD";
        }
    }
    if mnemonic.starts_with("CMP") {
        if mnemonic.ends_with("PS") {
            return "CMPPS";
        }
        if mnemonic.ends_with("PD") {
            return "CMPPD";
        }
        if mnemonic.ends_with("SS") {
            return "CMPSS";
        }
        if mnemonic.ends_with("SD") {
            return "CMPSD";
        }
    }
    match mnemonic {
        "JE" => "JZ",
        "JNE" => "JNZ",
        "CMOVNLE" => "CMOVG",
        "SETZ" => "SETE",
        _ => mnemonic,
    }
}

fn normalize_iform_prefix(iform: &str) -> String {
    normalize_mnemonic(iform.split('_').next().unwrap_or(""))
}
