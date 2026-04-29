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
