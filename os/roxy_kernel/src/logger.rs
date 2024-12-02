use super::{framebuffer::FrameBufferWriter, serial::SerialPort};
use bootloader_api::info::{FrameBuffer, FrameBufferInfo};
use conquer_once::spin::OnceCell;
use core::fmt::Write;
use log::LevelFilter;
use spinning_top::Spinlock;
use x86_64::instructions::interrupts;

/// The global logger instance used for the `log` crate.
pub static LOGGER: OnceCell<LockedLogger> = OnceCell::uninit();

/// A logger instance protected by a spinlock.
pub struct LockedLogger {
    framebuffer: Option<Spinlock<FrameBufferWriter>>,
    serial: Option<Spinlock<SerialPort>>,
}

impl LockedLogger {
    /// Create a new instance that logs to the given framebuffer.
    pub fn new(framebuffer: &'static mut [u8], info: FrameBufferInfo) -> Self {
        let framebuffer = Spinlock::new(FrameBufferWriter::new(framebuffer, info));
        let serial = Spinlock::new(unsafe { SerialPort::init() });

        LockedLogger {
            framebuffer: Some(framebuffer),
            serial: Some(serial),
        }
    }
}

impl log::Log for LockedLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if let Some(framebuffer) = &self.framebuffer {
            interrupts::without_interrupts(|| {
                let mut framebuffer = framebuffer.lock();
                writeln!(framebuffer, "{:5}: {}", record.level(), record.args()).unwrap();
            })
        }
        if let Some(serial) = &self.serial {
            interrupts::without_interrupts(|| {
                let mut serial = serial.lock();
                writeln!(serial, "{:5}: {}", record.level(), record.args()).unwrap();
            });
        }
    }

    fn flush(&self) {}
}

pub fn init(fb: FrameBuffer) {
    let info = fb.info();
    let logger = LOGGER.get_or_init(move || LockedLogger::new(fb.into_buffer(), info));

    log::set_logger(logger).expect("logger already set");
    log::set_max_level(if cfg!(debug_assertions) {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    });
}
