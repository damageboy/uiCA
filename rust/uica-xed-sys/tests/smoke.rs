use std::ffi::CStr;

use uica_xed_sys::{uica_xed_decode_one, uica_xed_init, uica_xed_inst_t, UICA_XED_STATUS_OK};

fn cstr(buf: &[std::os::raw::c_char]) -> String {
    unsafe { CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned() }
}

#[test]
fn decodes_add_rax_rbx() {
    let bytes = [0x48, 0x01, 0xd8];
    let mut inst = uica_xed_inst_t::default();

    unsafe {
        uica_xed_init();
        let rc = uica_xed_decode_one(bytes.as_ptr(), bytes.len() as u32, 0, &mut inst);
        assert_eq!(rc, 0);
    }

    assert_eq!(inst.status, UICA_XED_STATUS_OK);
    assert_eq!(inst.len, 3);
    assert_eq!(cstr(&inst.mnemonic), "add");
    assert_eq!(cstr(&inst.disasm), "add rax, rbx");
}
