#![no_std]
#![no_main]

use core::panic::PanicInfo;

use bootloader_api::info::Optional;
use log::LevelFilter;
use logger::LockedLogger;

mod framebuffer;
mod logger;
mod serial;

bootloader_api::entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut bootloader_api::BootInfo) -> ! {
    let mut fb = Optional::None;
    core::mem::swap(&mut fb, &mut boot_info.framebuffer);
    let fb = fb.into_option().unwrap();
    let info = fb.info();
    let logger = logger::LOGGER.get_or_init(move || {
        LockedLogger::new(
            fb.into_buffer(),
            info,
        )
    });

    log::set_logger(logger).expect("logger already set");
    log::set_max_level(
        if cfg!(debug_assertions) {
            LevelFilter::Debug
        } else {
            LevelFilter::Info
        }
    );


    log::info!("Hello, World!");

    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}