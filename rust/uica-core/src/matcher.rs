use uica_data::InstructionRecord;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NormalizedInstr {
    pub mnemonic: String,
    /// Operand-kind signature built from iced-x86 op kinds (e.g. `GPRv_GPRv`).
    /// Empty string when not known; matcher falls back to mnemonic-only.
    pub iform_signature: String,
    /// Maximum operand register size in bytes (e.g. 8=R64, 4=R32, 2=R16).
    /// Zero when not known. Used to disambiguate records whose iforms share
    /// the same signature (e.g. MOV_GPRv_GPRv_89 has R16/R32/R64 variants).
    pub max_op_size_bytes: u8,
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
            if max_size > 0 {
                let size_tag = match max_size {
                    8 => "R64",
                    4 => "R32",
                    2 => "R16",
                    1 => "R8",
                    _ => "",
                };
                if !size_tag.is_empty() {
                    if let Some(hit) = sig_matches.iter().find(|c| c.string.contains(size_tag)) {
                        return Some(hit);
                    }
                }
            }
            return sig_matches.into_iter().next();
        }
    }

    // Fallback: string mnemonic match, prefer by operand size if known.
    let string_matches: Vec<&InstructionRecord> = candidates
        .iter()
        .filter(|c| normalize_mnemonic(&c.string) == normalized_mnemonic)
        .collect();
    if !string_matches.is_empty() {
        if max_size > 0 {
            let size_tag = match max_size {
                8 => "R64",
                4 => "R32",
                2 => "R16",
                1 => "R8",
                _ => "",
            };
            if !size_tag.is_empty() {
                if let Some(hit) = string_matches.iter().find(|c| c.string.contains(size_tag)) {
                    return Some(hit);
                }
            }
        }
        return string_matches.into_iter().next();
    }
    candidates
        .iter()
        .find(|candidate| normalize_iform_prefix(&candidate.iform) == normalized_mnemonic)
}

/// True if the record's iform starts with `<mnemonic>_<signature>` (with an
/// optional `_<suffix>` allowed — uops.info adds disambiguators like `_11`).
/// Comparison is case-insensitive because uops.info iforms retain the
/// lowercase `v`/`z`/`w`/`b` size tags from Intel SDM (e.g. `ADC_GPRv_GPRv`).
fn iform_matches_signature(iform: &str, mnemonic: &str, signature: &str) -> bool {
    let expected_prefix = format!("{}_{}", mnemonic, signature);
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
    match mnemonic {
        "JNE" => "JNZ",
        "CMOVNLE" => "CMOVG",
        "SETZ" => "SETE",
        _ => mnemonic,
    }
}

fn normalize_iform_prefix(iform: &str) -> String {
    normalize_mnemonic(iform.split('_').next().unwrap_or(""))
}
