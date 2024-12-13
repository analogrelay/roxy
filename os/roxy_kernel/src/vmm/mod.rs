use x86_64::VirtAddr;

pub const KERNEL_IMAGE_START: VirtAddr = VirtAddr::new_truncate(0x8000_0000_0000);
pub const KERNEL_STACK_START: VirtAddr = VirtAddr::new_truncate(0x9000_0000_0000);
pub const KERNEL_HEAP_START: VirtAddr = VirtAddr::new_truncate(0xA000_0000_0000);
pub const PHYSICAL_MAP_START: VirtAddr = VirtAddr::new_truncate(0xC000_0000_0000);

mod memory_map;
pub use memory_map::*;

pub struct VirtualMemoryManager {}

impl VirtualMemoryManager {
    pub fn new(memory_map: Vec<MemoryRegion>) -> VirtualMemoryManager {}
}
