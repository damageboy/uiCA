use uica_xed::decode_raw;

#[test]
fn decodes_loop_add_bytes() {
    let bytes = [
        0x48_u8, 0x01, 0xd8, 0x48, 0x01, 0xc3, 0x49, 0xff, 0xcf, 0x75, 0xf7,
    ];
    let instructions = decode_raw(&bytes).expect("decode should succeed");

    assert_eq!(instructions.len(), 4);
    assert_eq!(instructions[0].mnemonic, "add");
    assert_eq!(instructions[0].disasm, "add rax, rbx");
    assert_eq!(instructions[3].mnemonic, "jne");
}

#[test]
fn preserves_memory_immediate_iform_signature() {
    let instructions =
        decode_raw(&[0x48_u8, 0x83, 0x44, 0x8b, 0x10, 0x05]).expect("decode should succeed");

    assert_eq!(instructions.len(), 1);
    assert_eq!(instructions[0].iform_signature, "MEMv_IMMb");
}

#[test]
fn maps_lea_agen_iform_signature_to_mem() {
    let instructions =
        decode_raw(&[0x48_u8, 0x8d, 0x44, 0x8b, 0x10]).expect("decode should succeed");

    assert_eq!(instructions.len(), 1);
    assert_eq!(instructions[0].iform_signature, "GPRv_MEM");
}

#[test]
fn decodes_extended_low8_register_names_like_xed() {
    let bytes = [0x41_u8, 0x0f, 0xb6, 0xcc];
    let instructions = decode_raw(&bytes).expect("decode should succeed");

    assert_eq!(instructions[0].input_regs, vec!["R12B"]);
    assert_eq!(instructions[0].output_regs, vec!["ECX"]);
    assert_eq!(instructions[0].explicit_reg_operands, vec!["ECX", "R12B"]);
    assert_eq!(instructions[0].max_op_size_bytes, 4);
}

#[test]
fn evex_default_k0_mask_is_not_reported_as_input() {
    // vmovdqu64 zmm0, zmmword ptr [rsi]
    let instructions =
        decode_raw(&[0x62_u8, 0xf1, 0xfe, 0x48, 0x6f, 0x06]).expect("decode should succeed");
    let inst = &instructions[0];

    assert_eq!(inst.disasm, "vmovdqu64 zmm0, zmmword ptr [rsi]");
    assert!(!inst.input_regs.iter().any(|reg| reg == "K0"));
    assert!(!inst.explicit_reg_operands.iter().any(|reg| reg == "K0"));
}

#[test]
fn decodes_memory_operands_and_immediates() {
    let bytes = [0x48_u8, 0x83, 0x44, 0x8b, 0x10, 0x05]; // add qword ptr [rbx+rcx*4+0x10], 5
    let instructions = decode_raw(&bytes).expect("decode should succeed");
    let inst = &instructions[0];

    assert_eq!(inst.mnemonic, "add");
    assert!(inst.has_memory_read);
    assert!(inst.has_memory_write);
    assert_eq!(inst.immediate, Some(5));
    assert_eq!(inst.mem_addrs.len(), 1);
    assert_eq!(inst.mem_addrs[0].base.as_deref(), Some("RBX"));
    assert_eq!(inst.mem_addrs[0].index.as_deref(), Some("RCX"));
    assert_eq!(inst.mem_addrs[0].scale, 4);
    assert_eq!(inst.mem_addrs[0].disp, 0x10);
}

#[test]
fn rip_relative_memory_base_is_not_input_register() {
    let bytes = [0x48_u8, 0x8b, 0x05, 0x34, 0x12, 0x00, 0x00]; // mov rax, qword ptr [rip+0x1234]
    let instructions = decode_raw(&bytes).expect("decode should succeed");
    let inst = &instructions[0];

    assert_eq!(inst.mem_addrs.len(), 1);
    assert_eq!(inst.mem_addrs[0].base.as_deref(), Some("RIP"));
    assert!(!inst.input_regs.iter().any(|reg| reg == "RIP"));
}

#[test]
fn explicit_reg_operands_preserve_duplicate_operands() {
    let instructions = decode_raw(&[0x89_u8, 0xc0]).expect("decode should succeed");

    assert_eq!(instructions[0].explicit_reg_operands, vec!["EAX", "EAX"]);
}

#[test]
fn decodes_lea_agen() {
    let bytes = [0x48_u8, 0x8d, 0x44, 0x8b, 0x10]; // lea rax, [rbx+rcx*4+0x10]
    let instructions = decode_raw(&bytes).expect("decode should succeed");

    assert_eq!(instructions[0].mnemonic, "lea");
    assert_eq!(instructions[0].agen.as_deref(), Some("B_IS_D8"));
}

#[test]
fn normalizes_xed_near_far_control_flow_mnemonics() {
    let ret = decode_raw(&[0xc2_u8, 0x10, 0x00]).expect("decode should succeed");
    let call = decode_raw(&[0xff_u8, 0xd0]).expect("decode should succeed");

    assert_eq!(ret[0].mnemonic, "ret");
    assert_eq!(call[0].mnemonic, "call");
}

#[test]
fn reports_decode_error_offset() {
    let err = decode_raw(&[0x0f_u8])
        .expect_err("truncated bytes should fail")
        .to_string();
    assert!(err.contains("byte offset 0"), "{err}");
}

#[test]
fn decodes_ud2_instruction() {
    let instructions = decode_raw(&[0x0f_u8, 0x0b]).expect("decode should succeed");

    assert_eq!(instructions.len(), 1);
    assert_eq!(instructions[0].mnemonic, "ud2");
}

#[test]
fn add_writes_flags_without_exposing_rflags_as_register() {
    let instructions = decode_raw(&[0x48_u8, 0x01, 0xd8]).expect("decode should succeed");
    let inst = &instructions[0];

    assert_eq!(inst.disasm, "add rax, rbx");
    assert_eq!(inst.output_regs, vec!["RAX"]);
    assert!(inst.writes_flags);
    assert!(!inst.input_regs.iter().any(|reg| reg == "RFLAGS"));
    assert!(!inst.output_regs.iter().any(|reg| reg == "RFLAGS"));
}

#[test]
fn adc_reads_flags_without_exposing_rflags_as_register() {
    let instructions = decode_raw(&[0x48_u8, 0x11, 0xd8]).expect("decode should succeed");
    let inst = &instructions[0];

    assert_eq!(inst.disasm, "adc rax, rbx");
    assert!(inst.reads_flags);
    assert!(!inst.input_regs.iter().any(|reg| reg == "RFLAGS"));
    assert!(!inst.output_regs.iter().any(|reg| reg == "RFLAGS"));
}

#[test]
fn stack_delta_uses_decoded_stack_operand_width() {
    let push = decode_raw(&[0x66_u8, 0x50]).expect("decode should succeed");
    let pop = decode_raw(&[0x66_u8, 0x58]).expect("decode should succeed");

    assert_eq!(push[0].disasm, "push ax");
    assert_eq!(push[0].implicit_rsp_change, -2);
    assert_eq!(pop[0].disasm, "pop ax");
    assert_eq!(pop[0].implicit_rsp_change, 2);
}

#[test]
fn push_explicit_rsp_memory_is_not_marked_implicit_stack_operand() {
    let instructions = decode_raw(&[0xff_u8, 0x34, 0x24]).expect("decode should succeed");
    let mem_addrs = &instructions[0].mem_addrs;

    assert_eq!(instructions[0].disasm, "push qword ptr [rsp]");
    assert_eq!(mem_addrs.len(), 2);
    assert_eq!(
        mem_addrs
            .iter()
            .filter(|mem| mem.is_implicit_stack_operand)
            .count(),
        1
    );
    assert!(mem_addrs
        .iter()
        .any(|mem| !mem.is_implicit_stack_operand && mem.base.as_deref() == Some("RSP")));
}

#[test]
fn pop_explicit_rsp_memory_is_not_marked_implicit_stack_operand() {
    let instructions = decode_raw(&[0x8f_u8, 0x04, 0x24]).expect("decode should succeed");
    let mem_addrs = &instructions[0].mem_addrs;

    assert_eq!(instructions[0].disasm, "pop qword ptr [rsp]");
    assert_eq!(mem_addrs.len(), 2);
    assert_eq!(
        mem_addrs
            .iter()
            .filter(|mem| mem.is_implicit_stack_operand)
            .count(),
        1
    );
    assert!(mem_addrs
        .iter()
        .any(|mem| !mem.is_implicit_stack_operand && mem.base.as_deref() == Some("RSP")));
}
