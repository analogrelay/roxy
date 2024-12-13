use conquer_once::spin::OnceCell;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

use crate::boot::gdt;

static IDT: OnceCell<InterruptDescriptorTable> = OnceCell::uninit();

pub fn init() {
    log::debug!("Initializing interrupts");

    let idt = IDT.get_or_init(|| {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.page_fault.set_handler_fn(page_fault_handler);
        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }
        idt
    });
    idt.load();
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    log::error!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) -> ! {
    panic!("DOUBLE FAULT 0x{:X}\n{:#?}", error_code, stack_frame);
}

extern "x86-interrupt" fn page_fault_handler(stack_frame: InterruptStackFrame, error_code: PageFaultErrorCode) {
    use x86_64::registers::control::Cr2;

    log::error!("PAGE FAULT");
    log::debug!(" Accessed Address: {:?}", Cr2::read());
    log::debug!(" Error Code: {:?}", error_code);
    log::debug!("{:#?}", stack_frame);
    panic!("PAGE FAULT");
}