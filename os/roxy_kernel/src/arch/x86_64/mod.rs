mod gdt;
mod idt;
mod memory;
mod pic;
mod userspace;

pub use memory::{
    PhysicalAddress, VirtualAddress, KERNEL_HEAP_START, KERNEL_IMAGE_START, KERNEL_STACK_START,
    PHYSICAL_MAP_START,
};

use bootloader_api::info::Optional;

use crate::logger;

pub fn kernel_main(boot_info: &'static mut bootloader_api::BootInfo) -> ! {
    let mut fb = Optional::None;
    core::mem::swap(&mut fb, &mut boot_info.framebuffer);
    let fb = fb
        .into_option()
        .expect("bootloader to have given us a framebuffer");
    let fb_addr = &fb.buffer()[0] as *const u8 as usize;
    logger::init(fb);
    log::debug!(
        "Framebuffer located at: {:#08X}",
        fb_addr ^ 0xFFFF_0000_0000_0000
    );
    log::debug!(
        "Received boot information, version {}.{}.{}",
        boot_info.api_version.version_major(),
        boot_info.api_version.version_minor(),
        boot_info.api_version.version_patch()
    );
    log::debug!(
        "Kernel loaded from {:#X} to {:#X}, mapped around {:#X}, size {} bytes",
        boot_info.kernel_addr,
        boot_info.kernel_addr + boot_info.kernel_len,
        kernel_main as *const () as usize,
        boot_info.kernel_len,
    );

    log::info!("Roxy is booting...");

    gdt::init();
    idt::init();

    let vmm = unsafe {
        let phys_offset = VirtualAddress::new(
            boot_info
                .physical_memory_offset
                .into_option()
                .expect("bootloader to have given us a physical memory mapping"),
        );

        memory::init(phys_offset, &boot_info.memory_regions)
    };

    for region in vmm.memory_map().regions() {
        log::debug!(
            "Memory region: {:#X} - {:#X} ({:?})",
            region.start,
            region.end,
            region.kind,
        );
    }

    log::info!(
        "Memory map initialized. {} known bytes, {} reserved bytes",
        vmm.memory_map().total_memory(),
        vmm.memory_map().reserved_memory()
    );

    pic::init();

    // run_usermode_test(&vmm);

    loop {
        x86_64::instructions::hlt();
    }
}

// fn run_usermode_test(vmm: &'static VirtualMemoryManager) {
//     // Create a page table for the new process
//     let mut address_space = vmm.create_address_space();
//     let func_addr = VirtualAddress::from_ptr(userspace::userspace_prog_1 as *const ());
//     let func_phys = vmm
//         .to_physical_address(func_addr)
//         .expect("the user space program should be mapped");
//     let func_frame = PhysFrame::containing_address(func_phys);
//     let func_offset = func_addr.as_u64() & 0xFFF;

//     let func_page_start = VirtualAddress::new(0x400000);
//     let func_virt = func_page_start + func_offset;
//     let func_page = Page::containing_address(func_page_start);

//     address_space.map_existing_frame(func_page, func_frame, PageTableFlags::USER_ACCESSIBLE);
// }
