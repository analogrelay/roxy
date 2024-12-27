//! Userspace test functions.

use core::arch::asm;

pub unsafe fn userspace_prog_1() {
    asm!(
        "
        nop
        nop
        nop
    "
    );
}
