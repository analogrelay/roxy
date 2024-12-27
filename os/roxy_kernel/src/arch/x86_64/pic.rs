use pic8259::ChainedPics;
use spinning_top::Spinlock;

use super::idt::InterruptIndex;

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: Spinlock<ChainedPics> =
    Spinlock::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

pub fn init() {
    unsafe {
        // SAFETY: We're the first to initialize the PICs
        let mut pics = PICS.lock();
        pics.initialize();

        // Unmask interrupts
        pics.write_masks(0, 0);
    }
    x86_64::instructions::interrupts::enable();
}

pub unsafe fn eoi(interrupt_id: InterruptIndex) {
    // SAFETY: Caller must ensure we're in the correct interrupt
    PICS.lock().notify_end_of_interrupt(interrupt_id as u8);
}
