use alloc::vec::Vec;
use x86_64::structures::paging::{PhysFrame, Size4KiB};

use crate::vmm::{Error, MemoryMap, MemoryRegion, MemoryRegionKind, Result};

use super::MemoryPurpose;

pub struct FrameAllocator {
    pub(super) memory_map: MemoryMap,

    // TODO: Just use the free frames themselves to store the free list.
    free_list: Vec<PhysFrame>,
}

impl FrameAllocator {
    pub fn new(memory_map: MemoryMap) -> Self {
        Self {
            memory_map,
            free_list: Vec::new(),
        }
    }

    pub fn for_purpose(
        &mut self,
        purpose: MemoryPurpose,
    ) -> impl x86_64::structures::paging::FrameAllocator<Size4KiB> + '_ {
        FrameAllocatorWithPurpose {
            allocator: self,
            purpose,
        }
    }

    pub fn release_frame(&mut self, frame: PhysFrame) {
        self.free_list.push(frame);
    }

    pub fn allocate_frame(&mut self, purpose: MemoryPurpose) -> Result<PhysFrame> {
        if let Some(frame) = self.free_list.pop() {
            // TODO: Update the memory map.
            Ok(frame)
        } else {
            self.allocate_new_frame(purpose)
        }
    }

    fn allocate_new_frame(&mut self, purpose: MemoryPurpose) -> Result<PhysFrame> {
        // Find a new frame that isn't in use
        let (idx, region) = self
            .memory_map
            .regions_mut()
            .iter_mut()
            .enumerate()
            .find(|(_, r)| r.kind == MemoryRegionKind::Usable)
            .ok_or(Error::OutOfPhysicalMemory)?;
        log::trace!("Allocating new frame from region {:?}", region);

        // Create a new region for the frame
        let frame = PhysFrame::from_start_address(region.start)
            .expect("region start address must be page aligned");
        let new_region = MemoryRegion::new(
            frame.start_address(),
            frame.start_address() + frame.size(),
            MemoryRegionKind::InUse(purpose),
        );

        // TODO: Updating the memory map is clunky, we could do better.

        if new_region.end < region.end {
            // Split the region
            let remainder = MemoryRegion::new(new_region.end, region.end, region.kind);
            *region = remainder;

            // And insert the new region
            self.memory_map.insert_region(idx, new_region);
        } else {
            // The region is fully consumed, just update the kind
            region.kind = MemoryRegionKind::InUse(purpose);
        }

        Ok(frame)
    }
}

struct FrameAllocatorWithPurpose<'a> {
    allocator: &'a mut FrameAllocator,
    purpose: MemoryPurpose,
}

unsafe impl x86_64::structures::paging::FrameAllocator<Size4KiB> for FrameAllocatorWithPurpose<'_> {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        self.allocator.allocate_frame(self.purpose).ok()
    }
}
