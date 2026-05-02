use uica_decoder::decode_raw;

#[test]
fn decodes_loop_add_bytes() {
    let bytes = [
        0x48_u8, 0x01, 0xd8, 0x48, 0x01, 0xc3, 0x49, 0xff, 0xcf, 0x75, 0xf7,
    ];
    let instructions = decode_raw(&bytes).expect("decode should succeed");

    assert_eq!(instructions.len(), 4);
    assert_eq!(
        instructions.first().map(|inst| inst.mnemonic.as_str()),
        Some("add")
    );
    assert_eq!(
        instructions.last().map(|inst| inst.mnemonic.as_str()),
        Some("jne")
    );
}

#[test]
fn decodes_extended_low8_register_names_like_xed() {
    let bytes = [0x41_u8, 0x0f, 0xb6, 0xcc]; // movzx ecx, r12b
    let instructions = decode_raw(&bytes).expect("decode should succeed");

    assert_eq!(instructions[0].input_regs, vec!["R12B"]);
    assert_eq!(instructions[0].explicit_reg_operands, vec!["ECX", "R12B"]);
}

#[test]
fn decoder_reexports_xed_lea_metadata() {
    let bytes = [0x48_u8, 0x8d, 0x44, 0x8b, 0x10];
    let instructions = decode_raw(&bytes).expect("decode should succeed");

    assert_eq!(instructions[0].mnemonic, "lea");
    assert_eq!(instructions[0].agen.as_deref(), Some("B_IS_D8"));
}

#[test]
fn decoder_exposes_lcp_inputs_for_prefix66_imm16() {
    let bytes = [0x66_u8, 0x81, 0x7e, 0x08, 0xf8, 0x00]; // cmp word ptr [rsi+8], 0xf8
    let instructions = decode_raw(&bytes).expect("decode should succeed");

    assert_eq!(instructions[0].mnemonic, "cmp");
    assert!(instructions[0].has_66_prefix);
    assert_eq!(instructions[0].immediate_width_bits, 16);
}
