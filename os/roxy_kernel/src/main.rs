#![no_std]
#![no_main]

use core::panic::PanicInfo;

use bootloader_api::config::Mapping;

static CONFIG: bootloader_api::BootloaderConfig = {
    let mut cfg = bootloader_api::BootloaderConfig::new_default();
    cfg.mappings.physical_memory = Some(Mapping::FixedAddress(
        roxy_kernel::vmm::PHYSICAL_MAP_START.as_u64(),
    ));
    cfg.mappings.kernel_stack =
        Mapping::FixedAddress(roxy_kernel::vmm::KERNEL_STACK_START.as_u64());
    cfg.mappings.dynamic_range_start = Some(0xB000_0000_0000);
    cfg
};

bootloader_api::entry_point!(roxy_kernel::boot::kernel_main, config = &CONFIG);

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
