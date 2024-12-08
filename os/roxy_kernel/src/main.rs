#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

use core::panic::PanicInfo;

use bootloader_api::{config::Mapping, info::{MemoryRegionKind, Optional}};

mod framebuffer;
mod gdt;
mod idt;
mod logger;
mod serial;
mod vmm;

static CONFIG: bootloader_api::BootloaderConfig = {
    let mut cfg = bootloader_api::BootloaderConfig::new_default();
    cfg.mappings.physical_memory = Some(Mapping::FixedAddress(vmm::PHYSICAL_MAP_START.as_u64()));
    cfg.mappings.kernel_stack = Mapping::FixedAddress(vmm::KERNEL_STACK_START.as_u64());
    cfg.mappings.dynamic_range_start = Some(0xA000_0000_0000);
    cfg
};

bootloader_api::entry_point!(kernel_main, config = &CONFIG);

fn kernel_main(boot_info: &'static mut bootloader_api::BootInfo) -> ! {
    let mut fb = Optional::None;
    core::mem::swap(&mut fb, &mut boot_info.framebuffer);
    let fb = fb.into_option().unwrap();
    logger::init(fb);
    log::debug!(
        "Received boot information, version {}.{}.{}",
        boot_info.api_version.version_major(),
        boot_info.api_version.version_minor(),
        boot_info.api_version.version_patch()
    );

    if cfg!(debug_assertions) {
        dump_boot_info(boot_info);
    }

    log::info!("Roxy is booting...");

    gdt::init();
    idt::init();

    todo!();
}

fn dump_boot_info(boot_info: &mut bootloader_api::BootInfo) {
    log::debug!("Kernel: {:#08x} - {:#08x} ({} bytes)", boot_info.kernel_image_offset, boot_info.kernel_image_offset + boot_info.kernel_len, boot_info.kernel_len);
    log::debug!("  Physical Addresses: {:#08x} - {:#08x}", boot_info.kernel_addr, boot_info.kernel_addr + boot_info.kernel_len);
    log::debug!("  Current instruction: {:#08x}", x86_64::registers::read_rip());
    log::debug!("  Stack value: {:p}", &boot_info);
    if let Optional::Some(o) = boot_info.physical_memory_offset {
        log::debug!("Physical Memory Offset: {:#08x}", o);
    }
    if let Optional::Some(o) = boot_info.rsdp_addr {
        if let Optional::Some(phys_offset) = boot_info.physical_memory_offset {
            log::debug!("RSDP Address: {:#08x} (Virtual Address: {:#08x})", o, o + phys_offset);
        } else {
            log::debug!("RSDP Address: {:#08x}", o);
        }
    }
    if let Optional::Some(t) = boot_info.tls_template {
        log::debug!("TLS template: {:#08x} ({} file size, {} mem size)", t.start_addr, t.file_size, t.mem_size);
    }
    if let Optional::Some(o) = boot_info.ramdisk_addr {
        log::debug!("Initial ramdisk: {:#08x} - {:#08x} ({} bytes)", o, o + boot_info.ramdisk_len, boot_info.ramdisk_len);
    }

    log::debug!("Reserved Memory Regions:");
    for mapping in boot_info.memory_regions.iter().filter(|r| r.kind != MemoryRegionKind::Usable) {
        if let MemoryRegionKind::UnknownUefi(i) = mapping.kind {
            let uefi_type = uefi::mem::memory_map::MemoryType(i);
            log::debug!("  UEFI({:?}): {:#08x} - {:#08x} ({} bytes)", uefi_type, mapping.start, mapping.end, mapping.end - mapping.start);
        } else {
            log::debug!("  {:?}: {:#08x} - {:#08x} ({} bytes)", mapping.kind, mapping.start, mapping.end, mapping.end - mapping.start);
        }
    }
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
