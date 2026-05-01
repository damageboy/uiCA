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
