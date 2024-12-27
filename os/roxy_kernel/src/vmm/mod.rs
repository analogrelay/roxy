use core::ops::Range;

use conquer_once::spin::OnceCell;
use thiserror::Error;
use x86_64::{
    structures::paging::{Mapper, OffsetPageTable, Page, PageTableFlags, PhysFrame, Translate},
    PhysAddr, VirtAddr,
};

mod address_space;
mod error;
mod frame_allocator;
mod memory_map;
pub use address_space::*;
pub use error::*;
pub use frame_allocator::*;
pub use memory_map::*;

use crate::arch::{PhysicalAddress, VirtualAddress, PHYSICAL_MAP_START};

static INSTANCE: OnceCell<VirtualMemoryManager> = OnceCell::uninit();

pub struct VirtualMemoryManager {
    kernel_page_table: OffsetPageTable<'static>,
    frame_allocator: FrameAllocator,
}

impl VirtualMemoryManager {
    pub fn init(
        memory_map: MemoryMap,
        page_table: OffsetPageTable<'static>,
    ) -> &'static VirtualMemoryManager {
        INSTANCE.get_or_init(|| Self {
            frame_allocator: FrameAllocator::new(memory_map),
            kernel_page_table: page_table,
        })
    }

    pub fn current() -> &'static VirtualMemoryManager {
        INSTANCE
            .get()
            .expect("VirtualMemoryManager not initialized")
    }

    pub fn create_address_space(&self) -> AddressSpace {
        AddressSpace::new(&self.kernel_page_table)
    }

    pub fn memory_map(&self) -> &MemoryMap {
        &self.frame_allocator.memory_map
    }

    /// Maps a physical region of memory to a virtual region
    pub fn map_physical_range(
        &self,
        range: Range<PhysicalAddress>,
    ) -> Result<Range<VirtualAddress>> {
        // We don't have to do any work for this, the bootloader already mapped all the physical memory for us.
        let range = VirtualAddress::new(range.start.as_u64() + PHYSICAL_MAP_START.as_u64())
            ..VirtualAddress::new(range.end.as_u64() + PHYSICAL_MAP_START.as_u64());
        Ok(range)
    }

    pub fn to_physical_address(&self, func_addr: VirtAddr) -> Option<PhysAddr> {
        self.kernel_page_table.translate_addr(func_addr)
    }
}
