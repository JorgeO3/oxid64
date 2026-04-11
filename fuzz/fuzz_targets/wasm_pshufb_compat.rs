#![no_main]

use libfuzzer_sys::fuzz_target;

fn pshufb_select_index(ctrl: u8) -> Option<usize> {
    if ctrl & 0x80 != 0 {
        None
    } else {
        Some((ctrl & 0x0f) as usize)
    }
}

fn wasm_swizzle_select_index(ctrl: u8) -> Option<usize> {
    if ctrl < 16 {
        Some(ctrl as usize)
    } else {
        None
    }
}

fn pshufb_lookup_byte(table: [u8; 16], ctrl: u8) -> u8 {
    match pshufb_select_index(ctrl) {
        Some(idx) => table[idx],
        None => 0,
    }
}

fuzz_target!(|data: &[u8]| {
    let table = *b"0123456789abcdef";

    for &ctrl in data.iter().take(256) {
        let got = pshufb_lookup_byte(table, ctrl);
        if ctrl & 0x80 != 0 {
            assert_eq!(got, 0);
            assert_eq!(pshufb_select_index(ctrl), None);
        } else {
            assert_eq!(got, table[(ctrl & 0x0f) as usize]);
            assert_eq!(pshufb_select_index(ctrl), Some((ctrl & 0x0f) as usize));
        }

        if ctrl < 16 {
            assert_eq!(wasm_swizzle_select_index(ctrl), Some(ctrl as usize));
        }
    }
});
