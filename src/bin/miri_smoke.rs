use std::hint::black_box;
use std::mem::MaybeUninit;

fn main() {
    // Intentionally invalid: this binary exists only to prove that Miri is
    // active and reports UB in a controlled way.
    let poisoned = MaybeUninit::<u8>::uninit();
    let value = unsafe { poisoned.assume_init() };
    black_box(value);
}
