// Full parity scaffold of Python MicroArchConfig. Fields mirror microArchConfigs.py.
// Unused fields are kept so the Rust simulator port can grow without schema churn.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveElimSlots {
    None,
    Finite(u32),
    Unlimited,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LsdUnrollEntry {
    pub nuops: u32,
    pub unroll: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MicroArchConfig {
    pub name: &'static str,
    pub xed_name: &'static str,

    // front-end queues
    pub iq_width: u32,
    pub dsb_width: u32,
    pub idq_width: u32,
    pub dsb_block_size: u32,
    pub predecode_width: u32,
    pub predecode_block_size: u32,
    pub predecode_decode_delay: u32,
    pub n_decoders: u32,
    pub dsb_ms_stall: u32,

    // back-end
    pub issue_width: u32,
    pub rb_width: u32,
    pub rs_width: u32,
    pub retire_width: u32,
    pub issue_dispatch_delay: u32,

    // decoder quirks
    pub pop5c_requires_complex_decoder: bool,
    pub pop5c_ends_decode_group: bool,
    pub macro_fusible_instr_can_be_decoded_as_last_instr: bool,
    pub branch_can_be_last_instr_in_cached_block: bool,
    pub both_32byte_blocks_must_be_cacheable: bool,

    // renamer quirks
    pub high8_renamed_separately: bool,
    pub movzx_high8_alias_can_be_eliminated: bool,
    pub move_elim_pipeline_length: u32,
    pub move_elim_gpr_slots: MoveElimSlots,
    pub move_elim_simd_slots: MoveElimSlots,
    pub move_elim_gpr_all_aliases_must_be_overwritten: bool,

    // loop stream detector
    pub lsd_enabled: bool,
    pub lsd_unrolling: &'static [LsdUnrollEntry],

    // scheduler / memory quirks
    pub fast_pointer_chasing: bool,
    pub slow_256bit_mem_acc: bool,
    pub simple_port_assignment: bool,
}

const EMPTY_LSD: &[LsdUnrollEntry] = &[];

const HSW_LSD: &[LsdUnrollEntry] = &[
    LsdUnrollEntry {
        nuops: 1,
        unroll: 8,
    },
    LsdUnrollEntry {
        nuops: 2,
        unroll: 8,
    },
    LsdUnrollEntry {
        nuops: 3,
        unroll: 8,
    },
    LsdUnrollEntry {
        nuops: 4,
        unroll: 8,
    },
    LsdUnrollEntry {
        nuops: 5,
        unroll: 6,
    },
    LsdUnrollEntry {
        nuops: 6,
        unroll: 5,
    },
    LsdUnrollEntry {
        nuops: 7,
        unroll: 4,
    },
    LsdUnrollEntry {
        nuops: 8,
        unroll: 4,
    },
    LsdUnrollEntry {
        nuops: 9,
        unroll: 3,
    },
    LsdUnrollEntry {
        nuops: 10,
        unroll: 3,
    },
    LsdUnrollEntry {
        nuops: 11,
        unroll: 3,
    },
    LsdUnrollEntry {
        nuops: 12,
        unroll: 3,
    },
    LsdUnrollEntry {
        nuops: 13,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 14,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 15,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 16,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 17,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 18,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 19,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 20,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 21,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 22,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 23,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 24,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 25,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 26,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 27,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 28,
        unroll: 2,
    },
];

const ICL_LSD: &[LsdUnrollEntry] = &[
    LsdUnrollEntry {
        nuops: 1,
        unroll: 6,
    },
    LsdUnrollEntry {
        nuops: 2,
        unroll: 6,
    },
    LsdUnrollEntry {
        nuops: 3,
        unroll: 6,
    },
    LsdUnrollEntry {
        nuops: 4,
        unroll: 6,
    },
    LsdUnrollEntry {
        nuops: 5,
        unroll: 6,
    },
    LsdUnrollEntry {
        nuops: 6,
        unroll: 6,
    },
    LsdUnrollEntry {
        nuops: 7,
        unroll: 4,
    },
    LsdUnrollEntry {
        nuops: 8,
        unroll: 4,
    },
    LsdUnrollEntry {
        nuops: 9,
        unroll: 3,
    },
    LsdUnrollEntry {
        nuops: 10,
        unroll: 3,
    },
    LsdUnrollEntry {
        nuops: 11,
        unroll: 3,
    },
    LsdUnrollEntry {
        nuops: 12,
        unroll: 3,
    },
    LsdUnrollEntry {
        nuops: 13,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 14,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 15,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 16,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 17,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 18,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 19,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 20,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 21,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 22,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 23,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 24,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 25,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 26,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 27,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 28,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 29,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 30,
        unroll: 2,
    },
];

const CLX_LSD: &[LsdUnrollEntry] = &[
    LsdUnrollEntry {
        nuops: 1,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 2,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 3,
        unroll: 6,
    },
    LsdUnrollEntry {
        nuops: 4,
        unroll: 6,
    },
    LsdUnrollEntry {
        nuops: 5,
        unroll: 6,
    },
    LsdUnrollEntry {
        nuops: 6,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 7,
        unroll: 3,
    },
    LsdUnrollEntry {
        nuops: 8,
        unroll: 3,
    },
    LsdUnrollEntry {
        nuops: 9,
        unroll: 3,
    },
    LsdUnrollEntry {
        nuops: 10,
        unroll: 3,
    },
    LsdUnrollEntry {
        nuops: 11,
        unroll: 3,
    },
    LsdUnrollEntry {
        nuops: 12,
        unroll: 3,
    },
    LsdUnrollEntry {
        nuops: 13,
        unroll: 3,
    },
    LsdUnrollEntry {
        nuops: 14,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 15,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 16,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 17,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 18,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 19,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 20,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 21,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 22,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 23,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 24,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 25,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 26,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 27,
        unroll: 2,
    },
    LsdUnrollEntry {
        nuops: 28,
        unroll: 2,
    },
];

fn skl() -> MicroArchConfig {
    MicroArchConfig {
        name: "SKL",
        xed_name: "SKYLAKE",
        iq_width: 25,
        dsb_width: 6,
        idq_width: 64,
        dsb_block_size: 32,
        predecode_width: 5,
        predecode_block_size: 16,
        predecode_decode_delay: 3,
        n_decoders: 4,
        dsb_ms_stall: 2,
        issue_width: 4,
        rb_width: 224,
        rs_width: 97,
        retire_width: 4,
        issue_dispatch_delay: 5,
        pop5c_requires_complex_decoder: true,
        pop5c_ends_decode_group: false,
        macro_fusible_instr_can_be_decoded_as_last_instr: true,
        branch_can_be_last_instr_in_cached_block: false,
        both_32byte_blocks_must_be_cacheable: true,
        high8_renamed_separately: true,
        movzx_high8_alias_can_be_eliminated: false,
        move_elim_pipeline_length: 2,
        move_elim_gpr_slots: MoveElimSlots::Finite(4),
        move_elim_simd_slots: MoveElimSlots::Finite(4),
        move_elim_gpr_all_aliases_must_be_overwritten: true,
        lsd_enabled: false,
        lsd_unrolling: EMPTY_LSD,
        fast_pointer_chasing: true,
        slow_256bit_mem_acc: false,
        simple_port_assignment: false,
    }
}

fn hsw() -> MicroArchConfig {
    MicroArchConfig {
        name: "HSW",
        xed_name: "HASWELL",
        iq_width: 20,
        dsb_width: 4,
        idq_width: 56,
        dsb_block_size: 32,
        predecode_width: 5,
        predecode_block_size: 16,
        predecode_decode_delay: 3,
        n_decoders: 4,
        dsb_ms_stall: 4,
        issue_width: 4,
        rb_width: 192,
        rs_width: 60,
        retire_width: 4,
        issue_dispatch_delay: 5,
        pop5c_requires_complex_decoder: true,
        pop5c_ends_decode_group: true,
        macro_fusible_instr_can_be_decoded_as_last_instr: false,
        branch_can_be_last_instr_in_cached_block: true,
        both_32byte_blocks_must_be_cacheable: false,
        high8_renamed_separately: true,
        movzx_high8_alias_can_be_eliminated: false,
        move_elim_pipeline_length: 2,
        move_elim_gpr_slots: MoveElimSlots::Finite(4),
        move_elim_simd_slots: MoveElimSlots::Finite(4),
        move_elim_gpr_all_aliases_must_be_overwritten: true,
        lsd_enabled: true,
        lsd_unrolling: HSW_LSD,
        fast_pointer_chasing: true,
        slow_256bit_mem_acc: false,
        simple_port_assignment: false,
    }
}

fn icl() -> MicroArchConfig {
    MicroArchConfig {
        name: "ICL",
        xed_name: "ICE_LAKE",
        iq_width: 25,
        dsb_width: 6,
        idq_width: 70,
        dsb_block_size: 64,
        predecode_width: 5,
        predecode_block_size: 16,
        predecode_decode_delay: 3,
        n_decoders: 4,
        dsb_ms_stall: 2,
        issue_width: 5,
        rb_width: 352,
        rs_width: 160,
        retire_width: 8,
        issue_dispatch_delay: 5,
        pop5c_requires_complex_decoder: true,
        pop5c_ends_decode_group: false,
        macro_fusible_instr_can_be_decoded_as_last_instr: true,
        branch_can_be_last_instr_in_cached_block: true,
        both_32byte_blocks_must_be_cacheable: false,
        high8_renamed_separately: false,
        movzx_high8_alias_can_be_eliminated: false,
        move_elim_pipeline_length: 2,
        move_elim_gpr_slots: MoveElimSlots::None,
        move_elim_simd_slots: MoveElimSlots::Unlimited,
        move_elim_gpr_all_aliases_must_be_overwritten: true,
        lsd_enabled: true,
        lsd_unrolling: ICL_LSD,
        fast_pointer_chasing: false,
        slow_256bit_mem_acc: false,
        simple_port_assignment: false,
    }
}

fn skx() -> MicroArchConfig {
    let mut cfg = skl();
    cfg.name = "SKX";
    cfg.xed_name = "SKYLAKE_SERVER";
    cfg
}

fn kbl() -> MicroArchConfig {
    let mut cfg = skl();
    cfg.name = "KBL";
    cfg
}

fn cfl() -> MicroArchConfig {
    let mut cfg = skl();
    cfg.name = "CFL";
    cfg
}

fn clx() -> MicroArchConfig {
    let mut cfg = skl();
    cfg.name = "CLX";
    cfg.xed_name = "CASCADE_LAKE";
    cfg.lsd_enabled = true;
    cfg.lsd_unrolling = CLX_LSD;
    cfg
}

fn bdw() -> MicroArchConfig {
    let mut cfg = hsw();
    cfg.name = "BDW";
    cfg.xed_name = "BROADWELL";
    cfg
}

fn ivb() -> MicroArchConfig {
    MicroArchConfig {
        name: "IVB",
        xed_name: "IVYBRIDGE",
        iq_width: 20,
        dsb_width: 4,
        idq_width: 56,
        dsb_block_size: 32,
        predecode_width: 5,
        predecode_block_size: 16,
        predecode_decode_delay: 3,
        n_decoders: 4,
        dsb_ms_stall: 4,
        issue_width: 4,
        rb_width: 168,
        rs_width: 54,
        retire_width: 4,
        issue_dispatch_delay: 5,
        pop5c_requires_complex_decoder: true,
        pop5c_ends_decode_group: true,
        macro_fusible_instr_can_be_decoded_as_last_instr: false,
        branch_can_be_last_instr_in_cached_block: true,
        both_32byte_blocks_must_be_cacheable: false,
        high8_renamed_separately: true,
        movzx_high8_alias_can_be_eliminated: true,
        move_elim_pipeline_length: 3,
        move_elim_gpr_slots: MoveElimSlots::Finite(4),
        move_elim_simd_slots: MoveElimSlots::Finite(4),
        move_elim_gpr_all_aliases_must_be_overwritten: false,
        lsd_enabled: true,
        lsd_unrolling: EMPTY_LSD,
        fast_pointer_chasing: true,
        slow_256bit_mem_acc: true,
        simple_port_assignment: false,
    }
}

fn snb() -> MicroArchConfig {
    let mut cfg = ivb();
    cfg.name = "SNB";
    cfg.xed_name = "SANDYBRIDGE";
    cfg.idq_width = 28;
    cfg
}

fn tgl() -> MicroArchConfig {
    let mut cfg = icl();
    cfg.name = "TGL";
    cfg.xed_name = "TIGER_LAKE";
    cfg
}

fn rkl() -> MicroArchConfig {
    let mut cfg = icl();
    cfg.name = "RKL";
    cfg.move_elim_gpr_slots = MoveElimSlots::Unlimited;
    cfg
}

fn clx_simple_ports() -> MicroArchConfig {
    let mut cfg = clx();
    cfg.name = "CLX_SimplePorts";
    cfg.simple_port_assignment = true;
    cfg
}

fn clx_no_lsd() -> MicroArchConfig {
    let mut cfg = clx();
    cfg.name = "CLX_noLSD";
    cfg.lsd_enabled = false;
    cfg
}

fn clx_no_lsd_unrolling() -> MicroArchConfig {
    let mut cfg = clx();
    cfg.name = "CLX_noLSDUnrolling";
    cfg.lsd_unrolling = EMPTY_LSD;
    cfg
}

fn clx_no_move_elim() -> MicroArchConfig {
    let mut cfg = clx();
    cfg.name = "CLX_noMoveElim";
    cfg.move_elim_gpr_slots = MoveElimSlots::None;
    cfg.move_elim_simd_slots = MoveElimSlots::None;
    cfg
}

fn clx_full_move_elim() -> MicroArchConfig {
    let mut cfg = clx();
    cfg.name = "CLX_fullMoveElim";
    cfg.move_elim_gpr_slots = MoveElimSlots::Unlimited;
    cfg.move_elim_simd_slots = MoveElimSlots::Unlimited;
    cfg
}

fn clx_simple_ports_no_move_elim() -> MicroArchConfig {
    let mut cfg = clx_no_move_elim();
    cfg.name = "CLX_SimplePorts_noMoveElim";
    cfg.simple_port_assignment = true;
    cfg
}

pub fn get_micro_arch(name: &str) -> Option<MicroArchConfig> {
    let upper = name.to_ascii_uppercase();
    match upper.as_str() {
        "SNB" => Some(snb()),
        "IVB" => Some(ivb()),
        "HSW" => Some(hsw()),
        "BDW" => Some(bdw()),
        "SKL" => Some(skl()),
        "SKX" => Some(skx()),
        "KBL" => Some(kbl()),
        "CFL" => Some(cfl()),
        "CLX" => Some(clx()),
        "ICL" => Some(icl()),
        "TGL" => Some(tgl()),
        "RKL" => Some(rkl()),
        "CLX_SIMPLEPORTS" => Some(clx_simple_ports()),
        "CLX_NOLSD" => Some(clx_no_lsd()),
        "CLX_NOLSDUNROLLING" => Some(clx_no_lsd_unrolling()),
        "CLX_NOMOVEELIM" => Some(clx_no_move_elim()),
        "CLX_FULLMOVEELIM" => Some(clx_full_move_elim()),
        "CLX_SIMPLEPORTS_NOMOVEELIM" => Some(clx_simple_ports_no_move_elim()),
        _ => None,
    }
}
