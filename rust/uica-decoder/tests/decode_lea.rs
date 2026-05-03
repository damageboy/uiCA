use uica_decoder::decode_raw;

#[test]
fn decodes_lea_agen_and_address_registers() {
    let bytes = [0x48_u8, 0x8d, 0x44, 0x4b, 0x08];
    let instructions = decode_raw(&bytes).expect("decode should succeed");
    let lea = &instructions[0];

    assert_eq!(lea.mnemonic, "lea");
    assert_eq!(lea.iform_signature, "GPRv_MEM");
    assert_eq!(lea.agen.as_deref(), Some("B_IS_D8"));
    assert!(!lea.has_memory_read);
    assert_eq!(lea.mem_addrs.len(), 1);
    assert_eq!(lea.mem_addrs[0].base.as_deref(), Some("RBX"));
    assert_eq!(lea.mem_addrs[0].index.as_deref(), Some("RCX"));
    assert_eq!(lea.mem_addrs[0].scale, 2);
    assert_eq!(lea.mem_addrs[0].disp, 8);
}
