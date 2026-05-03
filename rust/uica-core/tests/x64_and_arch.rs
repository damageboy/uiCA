use uica_core::{get_canonical_reg, get_micro_arch};

#[test]
fn canonicalizes_gp_register_aliases() {
    assert_eq!(get_canonical_reg("EAX"), "RAX");
    assert_eq!(get_canonical_reg("AL"), "RAX");
    assert_eq!(get_canonical_reg("R9D"), "R9");
}

#[test]
fn canonicalizes_vector_register_aliases() {
    assert_eq!(get_canonical_reg("YMM3"), "XMM3");
    assert_eq!(get_canonical_reg("ZMM15"), "XMM15");
}

#[test]
fn returns_skl_basic_config() {
    let arch = get_micro_arch("SKL").expect("SKL config should exist");

    assert_eq!(arch.issue_width, 4);
    assert_eq!(arch.idq_width, 64);
}

#[test]
fn returns_hsw_and_icl_basic_configs() {
    let hsw = get_micro_arch("HSW").expect("HSW config should exist");
    let icl = get_micro_arch("ICL").expect("ICL config should exist");

    assert_eq!(hsw.issue_width, 4);
    assert_eq!(hsw.idq_width, 56);
    assert_eq!(icl.issue_width, 5);
    assert_eq!(icl.idq_width, 70);
}

#[test]
fn returns_none_for_unknown_arch() {
    assert!(get_micro_arch("NOPE").is_none());
}
