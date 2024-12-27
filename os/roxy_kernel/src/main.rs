#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[cfg(target_arch = "x86_64")]
static CONFIG: bootloader_api::BootloaderConfig = {
    use bootloader_api::config::Mapping;
    use roxy_kernel::arch::{KERNEL_STACK_START, PHYSICAL_MAP_START};

    let mut cfg = bootloader_api::BootloaderConfig::new_default();
    cfg.mappings.physical_memory = Some(Mapping::FixedAddress(PHYSICAL_MAP_START.as_u64()));
    cfg.mappings.kernel_stack = Mapping::FixedAddress(KERNEL_STACK_START.as_u64());
    cfg.mappings.dynamic_range_start = Some(0xB000_0000_0000);
    cfg
};

#[cfg(target_arch = "x86_64")]
bootloader_api::entry_point!(roxy_kernel::arch::kernel_main, config = &CONFIG);

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
