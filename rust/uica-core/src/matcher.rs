use uica_data::InstructionRecord;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NormalizedInstr {
    pub mnemonic: String,
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
    /// XED `agen` attribute for LEA addressing forms (e.g. B_IS_D8).
    pub agen: Option<String>,
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
    let normalized_mnemonic = normalize_mnemonic(&instruction.mnemonic);
    let sig = instruction.iform_signature.trim();

    // Prefer candidates whose iform carries the same operand-kind signature,
    // e.g. ADC_GPRv_GPRv_11 for a register/register ADC.
    let max_size = instruction.max_op_size_bytes;
    if !sig.is_empty() {
        let sig_matches: Vec<&InstructionRecord> = candidates
            .iter()
            .filter(|candidate| {
                iform_matches_signature(&candidate.iform, &normalized_mnemonic, sig)
            })
            .collect();
        if !sig_matches.is_empty() {
            return best_record_match(
                sig_matches,
                max_size,
                instruction.immediate,
                instruction.uses_high8_reg,
                &instruction.explicit_reg_operands,
                instruction.agen.as_deref(),
            );
        }
    }

    // Fallback: string mnemonic match, prefer by operand size if known.
    let string_matches: Vec<&InstructionRecord> = candidates
        .iter()
        .filter(|c| normalize_mnemonic(&c.string) == normalized_mnemonic)
        .collect();
    if !string_matches.is_empty() {
        return best_record_match(
            string_matches,
            max_size,
            instruction.immediate,
            instruction.uses_high8_reg,
            &instruction.explicit_reg_operands,
            instruction.agen.as_deref(),
        );
    }
    let iform_matches: Vec<&InstructionRecord> = candidates
        .iter()
        .filter(|candidate| normalize_iform_prefix(&candidate.iform) == normalized_mnemonic)
        .collect();
    best_record_match(
        iform_matches,
        max_size,
        instruction.immediate,
        instruction.uses_high8_reg,
        &instruction.explicit_reg_operands,
        instruction.agen.as_deref(),
    )
}

fn best_record_match<'a>(
    candidates: Vec<&'a InstructionRecord>,
    max_size: u8,
    immediate: Option<i64>,
    uses_high8_reg: bool,
    explicit_reg_operands: &[String],
    agen: Option<&str>,
) -> Option<&'a InstructionRecord> {
    let size_tag = match max_size {
        8 => "R64",
        4 => "R32",
        2 => "R16",
        1 => "R8",
        _ => "",
    };
    let sized: Vec<&InstructionRecord> = if size_tag.is_empty() {
        candidates
    } else {
        let filtered: Vec<&InstructionRecord> = candidates
            .iter()
            .copied()
            .filter(|c| c.string.contains(size_tag))
            .collect();
        if filtered.is_empty() {
            candidates
        } else {
            filtered
        }
    };

    let sized: Vec<&InstructionRecord> = if let Some(agen) = agen {
        let lea_prefix = format!("LEA_{agen} ");
        let filtered: Vec<&InstructionRecord> = sized
            .iter()
            .copied()
            .filter(|c| c.string.starts_with(&lea_prefix))
            .collect();
        if filtered.is_empty() {
            sized
        } else {
            filtered
        }
    } else {
        sized
    };

    let sized: Vec<&InstructionRecord> = if sized
        .iter()
        .any(|c| c.string.contains("R8h") || c.string.contains("R8l"))
    {
        let explicit_tags = explicit_r8_tags(explicit_reg_operands);
        let filtered: Vec<&InstructionRecord> = if explicit_tags.is_empty() {
            sized
                .iter()
                .copied()
                .filter(|c| {
                    if uses_high8_reg {
                        c.string.contains("R8h")
                    } else {
                        c.string.contains("R8l") && !c.string.contains("R8h")
                    }
                })
                .collect()
        } else {
            sized
                .iter()
                .copied()
                .filter(|c| record_r8_tags(&c.string) == explicit_tags)
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
        let has_immzero_metadata = sized.iter().any(|c| c.imm_zero);
        let is_zero_record = |record: &InstructionRecord| {
            record.imm_zero
                || (!has_immzero_metadata && legacy_zero_immediate_string(&record.string))
        };
        if imm == 0 {
            if let Some(hit) = sized.iter().find(|c| is_zero_record(c)) {
                return Some(*hit);
            }
        } else if let Some(hit) = sized.iter().find(|c| !is_zero_record(c)) {
            return Some(*hit);
        }
    }

    sized.into_iter().next()
}

fn legacy_zero_immediate_string(string: &str) -> bool {
    string.contains(", 0)") || string.contains("(0)") || string.contains("(0,")
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
    let upper = text
        .trim()
        .split(|ch: char| ch.is_whitespace() || ch == '(')
        .find(|part| !part.is_empty())
        .unwrap_or("")
        .to_ascii_uppercase();

    canonical_mnemonic_alias(upper.as_str()).to_string()
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
