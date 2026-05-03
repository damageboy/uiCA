pub const HIGH8_REGS: &[&str] = &["AH", "BH", "CH", "DH"];

pub const GP_REGS: &[&str] = &[
    "RAX", "RBX", "RCX", "RDX", "RSP", "RBP", "RSI", "RDI", "R8", "R9", "R10", "R11", "R12", "R13",
    "R14", "R15",
];

pub fn get_reg_size(reg: &str) -> u32 {
    let upper = reg.to_ascii_uppercase();
    match upper.as_str() {
        "AL" | "AH" | "BL" | "BH" | "CL" | "CH" | "DL" | "DH" | "SPL" | "BPL" | "SIL" | "DIL"
        | "R8B" | "R9B" | "R10B" | "R11B" | "R12B" | "R13B" | "R14B" | "R15B" => 8,
        "AX" | "BX" | "CX" | "DX" | "SP" | "BP" | "SI" | "DI" | "R8W" | "R9W" | "R10W" | "R11W"
        | "R12W" | "R13W" | "R14W" | "R15W" => 16,
        "EAX" | "EBX" | "ECX" | "EDX" | "ESP" | "EBP" | "ESI" | "EDI" | "R8D" | "R9D" | "R10D"
        | "R11D" | "R12D" | "R13D" | "R14D" | "R15D" => 32,
        _ if upper.starts_with("XMM") => 128,
        _ if upper.starts_with("YMM") => 256,
        _ if upper.starts_with("ZMM") => 512,
        _ if upper.starts_with('K') => 64,
        _ => 64,
    }
}

pub fn is_high8_reg(reg: &str) -> bool {
    HIGH8_REGS.contains(&reg.to_ascii_uppercase().as_str())
}

pub fn is_gp_reg(reg: &str) -> bool {
    GP_REGS.contains(&get_canonical_reg(reg).as_str())
}

pub fn get_canonical_reg(reg: &str) -> String {
    let upper = reg.to_ascii_uppercase();

    match upper.as_str() {
        "AH" | "AL" | "AX" | "EAX" | "RAX" => "RAX".to_string(),
        "BH" | "BL" | "BX" | "EBX" | "RBX" => "RBX".to_string(),
        "CH" | "CL" | "CX" | "ECX" | "RCX" => "RCX".to_string(),
        "DH" | "DL" | "DX" | "EDX" | "RDX" => "RDX".to_string(),
        "SPL" | "SP" | "ESP" | "RSP" => "RSP".to_string(),
        "BPL" | "BP" | "EBP" | "RBP" => "RBP".to_string(),
        "SIL" | "SI" | "ESI" | "RSI" => "RSI".to_string(),
        "DIL" | "DI" | "EDI" | "RDI" => "RDI".to_string(),
        "R8B" | "R8W" | "R8D" | "R8" => "R8".to_string(),
        "R9B" | "R9W" | "R9D" | "R9" => "R9".to_string(),
        "R10B" | "R10W" | "R10D" | "R10" => "R10".to_string(),
        "R11B" | "R11W" | "R11D" | "R11" => "R11".to_string(),
        "R12B" | "R12W" | "R12D" | "R12" => "R12".to_string(),
        "R13B" | "R13W" | "R13D" | "R13" => "R13".to_string(),
        "R14B" | "R14W" | "R14D" | "R14" => "R14".to_string(),
        "R15B" | "R15W" | "R15D" | "R15" => "R15".to_string(),
        _ if upper.starts_with("YMM") || upper.starts_with("ZMM") => {
            format!("XMM{}", &upper[3..])
        }
        _ => upper,
    }
}
