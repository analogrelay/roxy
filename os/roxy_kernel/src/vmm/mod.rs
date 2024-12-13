use x86_64::{registers::control::Cr3, structures::paging::{OffsetPageTable, PageTable}, VirtAddr};

pub const KERNEL_IMAGE_START: VirtAddr = VirtAddr::new_truncate(0x8000_0000_0000);
pub const KERNEL_STACK_START: VirtAddr = VirtAddr::new_truncate(0x9000_0000_0000);
pub const KERNEL_HEAP_START: VirtAddr = VirtAddr::new_truncate(0xA000_0000_0000);
pub const PHYSICAL_MAP_START: VirtAddr = VirtAddr::new_truncate(0xB000_0000_0000);

pub struct VirtualMemoryManager {
}

impl VirtualMemoryManager {
    pub fn init() -> Self {
        let page_table = OffsetPageTable::new(level_4_table, phys_offset)
    }
}

unsafe fn get_page_table() -> &'static mut PageTable {
    let (l4_table_frame, _) = Cr3::read();
    let phys = l4_table_frame.start_address();
}