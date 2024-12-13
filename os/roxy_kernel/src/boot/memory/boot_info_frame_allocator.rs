use alloc::vec::Vec;
use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
use x86_64::{
    structures::paging::{FrameAllocator, PhysFrame, Size4KiB},
    PhysAddr,
};

use crate::vmm;

/// Simple allocation-free frame allocator used to map the first heap pages
///
/// Once we have a heap, we move to the full [`KernelFrameAllocator`](crate::vmm::KernelFrameAllocator).
pub struct BootInfoFrameAllocator {
    memory_map: &'static MemoryRegions,
    next: usize,
}

impl BootInfoFrameAllocator {
    /// Create a FrameAllocator from the passed memory map.
    ///
    /// This function is unsafe because the caller must guarantee that the passed
    /// memory map is valid. The main requirement is that all frames that are marked
    /// as `USABLE` in it are really unused.
    pub unsafe fn init(memory_map: &'static MemoryRegions) -> Self {
        BootInfoFrameAllocator {
            memory_map,
            next: 0,
        }
    }

    /// Builds a [`vmm::MemoryMap`] using the provided bootloader map, and marks any currently-used frames.
    pub unsafe fn into_memory_map(mut self) -> vmm::MemoryMap {
        // This should only be called once the Kernel Heap is established.
        // We can now create a list of the frames that we allocated
        let end = self.next;
        self.next = 0;
        let usable_frames = self.usable_frames();
        let mut used_frames = usable_frames.take(end).collect::<Vec<_>>();
        used_frames.sort();

        let mut map_builder = vmm::MemoryMap::builder();
        let mut used_frame_iter = used_frames.into_iter();
        let mut next_used_frame = used_frame_iter.next();

        fn contains(region: &MemoryRegion, frame: PhysFrame) -> bool {
            region.start.as_u64() <= frame.start_address().as_u64()
                && region.end.as_u64() >= frame.start_address().as_u64()
        }

        for (i, region) in self.memory_map.iter().enumerate() {
            let mut region_start = region.start.as_u64();
            let mut region_end = region.end.as_u64();

            // Check if the next used frame is part of this region
            while let Some(used_frame) = next_used_frame
                && contains(region, used_frame)
            {
                // Split the region into two parts, before and after the used frame
                region_start = region.start.as_u64();
                region_end = used_frame.start_address().as_u64() - 1;

                // Is there anything left in the first part? It's possible the used frame was right at the start
                if region_start < region_end {
                    // Register this region
                    map_builder.add_region(
                        PhysAddr::new(region_start),
                        PhysAddr::new(region_end),
                        region.kind.into(),
                    );
                }
            }
        }

        map_builder.build()
    }

    /// Returns an iterator over the usable frames specified in the memory map.
    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        // get usable regions from memory map
        let regions = self.memory_map.iter();
        let usable_regions = regions.filter(|r| r.kind == MemoryRegionKind::Usable);
        // map each region to its address range
        let addr_ranges = usable_regions.map(|r| r.start..r.end);
        // transform to an iterator of frame start addresses
        let frame_addresses = addr_ranges.flat_map(|r| r.step_by(4096));
        // create `PhysFrame` types from the start addresses
        frame_addresses.map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        log::debug!("Allocated {} frames", self.next);
        frame
    }
}
