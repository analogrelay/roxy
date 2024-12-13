use core::panic::PanicInfo;

use alloc::{boxed::Box, rc::Rc, vec, vec::Vec};
use bootloader_api::{
    config::Mapping,
    info::{MemoryRegionKind, Optional},
};
use x86_64::VirtAddr;

use crate::vmm::{self, VirtualMemoryManager};

mod framebuffer;
mod gdt;
mod idt;
mod logger;
mod memory;
mod serial;

static CONFIG: bootloader_api::BootloaderConfig = {
    let mut cfg = bootloader_api::BootloaderConfig::new_default();
    cfg.mappings.physical_memory = Some(Mapping::FixedAddress(vmm::PHYSICAL_MAP_START.as_u64()));
    cfg.mappings.kernel_stack = Mapping::FixedAddress(vmm::KERNEL_STACK_START.as_u64());
    cfg.mappings.dynamic_range_start = Some(0xB000_0000_0000);
    cfg
};

bootloader_api::entry_point!(kernel_main, config = &CONFIG);

fn kernel_main(boot_info: &'static mut bootloader_api::BootInfo) -> ! {
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

    log::info!("Roxy is booting...");

    gdt::init();
    idt::init();

    unsafe {
        let phys_offset = VirtAddr::new(
            boot_info
                .physical_memory_offset
                .into_option()
                .expect("bootloader to have given us a physical memory mapping"),
        );

        memory::init(phys_offset, &boot_info.memory_regions);
    };

    // Now that we have a heap, build up the memory manager.
    let vmm = VirtualMemoryManager::new(&boot_info.memory_regions);

    todo!();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if let Some(loc) = info.location() {
        log::error!(
            "PANIC ({}:{}:{}): {:#?}",
            loc.file(),
            loc.line(),
            loc.column(),
            info.message()
        );
    } else {
        log::error!("PANIC (<unknown>): {:#?}", info.message());
    }

    loop {}
}
