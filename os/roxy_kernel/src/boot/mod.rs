use bootloader_api::{config::Mapping, info::Optional};
use x86_64::VirtAddr;

use crate::vmm;

mod framebuffer;
mod gdt;
mod idt;
mod logger;
mod memory;
mod serial;

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

    log::info!("Roxy is booting...");

    gdt::init();
    idt::init();

    let memory_map = unsafe {
        let phys_offset = VirtAddr::new(
            boot_info
                .physical_memory_offset
                .into_option()
                .expect("bootloader to have given us a physical memory mapping"),
        );

        memory::init(phys_offset, &boot_info.memory_regions)
    };

    log::info!(
        "Memory map initialized. {} known bytes, {} reserved bytes",
        memory_map.total_memory(),
        memory_map.reserved_memory()
    );

    if log::log_enabled!(log::Level::Debug) {
        for region in memory_map.regions() {
            log::debug!(
                "Region: {:#08X} - {:#08X} ({} bytes - {:?})",
                region.start,
                region.end,
                region.size(),
                region.kind,
            );
        }
    }

    todo!();
}
