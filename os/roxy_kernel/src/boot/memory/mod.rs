mod boot_info_frame_allocator;

use boot_info_frame_allocator::BootInfoFrameAllocator;
use bootloader_api::info::MemoryRegions;
use x86_64::{
    registers::control::Cr3,
    structures::paging::{
        FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, Size4KiB,
    },
    VirtAddr,
};

use crate::{heap::ALLOCATOR, vmm};

pub unsafe fn init(
    physical_offset: VirtAddr,
    memory_map: &'static MemoryRegions,
) -> vmm::MemoryMap {
    let mut page_table = get_page_table(physical_offset);

    // Clear the mappings below the kernel start, we don't need them.
    let pre_kernel_mappings = page_table
        .level_4_table_mut()
        .iter_mut()
        .enumerate()
        .take_while(|(i, _)| *i < 0o400);
    for (_, entry) in pre_kernel_mappings {
        entry.set_unused();
    }

    let mut frame_allocator = unsafe {
        // SAFETY: We trust the memory map provided by the bootloader
        BootInfoFrameAllocator::init(memory_map)
    };

    initialize_heap(&mut page_table, &mut frame_allocator);

    // Consume our current frame allocator and use it to build a memory map.
    frame_allocator.into_memory_map()
}

const INITIAL_HEAP_SIZE: usize = 100 * 1024;
fn initialize_heap(
    page_table: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) {
    // Start with a 100KiB heap.
    let heap_end = vmm::KERNEL_HEAP_START + INITIAL_HEAP_SIZE as u64;
    let start_page = Page::<Size4KiB>::containing_address(vmm::KERNEL_HEAP_START);
    let end_page = Page::<Size4KiB>::containing_address(heap_end);
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
    for page in Page::range_inclusive(start_page, end_page) {
        let frame = frame_allocator
            .allocate_frame()
            .expect("to have frames available");
        unsafe {
            // SAFETY: We're allocating a fresh frame we just acquired.
            page_table
                .map_to(page, frame, flags, frame_allocator)
                .expect("to be able to map a frame")
                .flush();
        };
    }

    unsafe {
        // SAFETY: We just allocated these pages.
        let mut alloc = ALLOCATOR.lock();
        alloc.init(vmm::KERNEL_HEAP_START.as_mut_ptr(), INITIAL_HEAP_SIZE);
        log::debug!(
            "Initialized Kernel Heap from {:p} - {:p}",
            alloc.bottom(),
            alloc.top(),
        );
    }
}

pub unsafe fn get_page_table(physical_offset: VirtAddr) -> OffsetPageTable<'static> {
    let (l4_table_frame, _) = Cr3::read();
    let phys = l4_table_frame.start_address();
    let virt = physical_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    // SAFETY: Given that the physical_offset is accurate (which the caller is asserting by calling us), this is safe.
    let table = unsafe { &mut *page_table_ptr };
    OffsetPageTable::new(table, physical_offset)
}
