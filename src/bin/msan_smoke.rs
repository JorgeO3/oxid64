use core::ffi::c_void;
use std::alloc::{Layout, alloc, dealloc};
use std::hint::black_box;

unsafe extern "C" {
    fn memcmp(lhs: *const c_void, rhs: *const c_void, n: usize) -> i32;
}

fn main() {
    let layout = Layout::from_size_align(32, 1).expect("valid layout");
    let rhs = [0u8; 32];

    // This binary is intentionally negative: it is used only to confirm that
    // MemorySanitizer instrumentation is active and reports reads from
    // uninitialized memory in a controlled way.
    unsafe {
        let lhs = alloc(layout);
        assert!(!lhs.is_null(), "allocation failed");

        let rc = memcmp(
            lhs.cast::<c_void>(),
            rhs.as_ptr().cast::<c_void>(),
            rhs.len(),
        );
        black_box(rc);

        dealloc(lhs, layout);
    }
}
