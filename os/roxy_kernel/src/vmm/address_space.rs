use alloc::boxed::Box;
use x86_64::structures::paging::{
    Mapper, OffsetPageTable, Page, PageSize, PageTable, PageTableFlags, PhysFrame,
};

pub struct AddressSpace {
    page_table: OffsetPageTable<'static>,
}

impl AddressSpace {
    pub(super) fn new(base_page_table: &OffsetPageTable) -> Self {
        // Make a copy of the base page table, but clear out every entry below the kernel start.
        // TODO: Don't leak this box.
        let page_table = Box::leak(Box::new(base_page_table.level_4_table().clone()));
        for i in 0..0o400 {
            page_table[i].set_unused();
        }
        let page_table = unsafe { OffsetPageTable::new(page_table, base_page_table.phys_offset()) };

        AddressSpace { page_table }
    }

    pub fn map_existing_frame<S: PageSize>(
        &self,
        page: Page<S>,
        frame: PhysFrame<S>,
        mut flags: PageTableFlags,
    ) {
        flags.set(PageTableFlags::PRESENT, true);
        let flusher = self.page_table.map_to(page, frame, flags, frame_allocator);
    }
}
