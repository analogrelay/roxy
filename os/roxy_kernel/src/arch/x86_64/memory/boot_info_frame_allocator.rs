use bootloader_api::info::{MemoryRegion, MemoryRegionKind, MemoryRegions};
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

impl From<MemoryRegionKind> for vmm::MemoryRegionKind {
    fn from(value: MemoryRegionKind) -> Self {
        match value {
            MemoryRegionKind::Usable => vmm::MemoryRegionKind::Usable,
            MemoryRegionKind::Bootloader => {
                vmm::MemoryRegionKind::Reserved(vmm::ReservedMemoryKind::ReservedByBootloader)
            }
            MemoryRegionKind::UnknownBios(value) => {
                vmm::MemoryRegionKind::Reserved(vmm::ReservedMemoryKind::ReservedByBios(value))
            }
            MemoryRegionKind::UnknownUefi(value) => {
                vmm::MemoryRegionKind::Reserved(vmm::ReservedMemoryKind::ReservedByUefi(value))
            }
            // Assume other non-usable memory is reserved
            _ => vmm::MemoryRegionKind::Reserved(vmm::ReservedMemoryKind::Unknown),
        }
    }
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
        let used_frames = usable_frames.take(end);
        build_memory_map(self.memory_map.iter(), used_frames)
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
        frame
    }
}

fn build_memory_map<'a>(
    memory_map: impl Iterator<Item = &'a MemoryRegion>,
    mut used_frames: impl Iterator<Item = PhysFrame>,
) -> vmm::MemoryMap {
    let mut map_builder = vmm::MemoryMap::builder();
    let mut current_frame = used_frames.next();
    for region in memory_map {
        let mut candidate = vmm::MemoryRegion::new(
            PhysAddr::new(region.start),
            PhysAddr::new(region.end),
            region.kind.into(),
        );

        while let Some(used_frame) = current_frame {
            if used_frame.start_address() >= candidate.end {
                break;
            }

            let region = vmm::MemoryRegion::new(
                used_frame.start_address(),
                used_frame.start_address() + used_frame.size(),
                vmm::MemoryRegionKind::InUse(vmm::MemoryPurpose::KernelHeap),
            );
            match candidate.try_merge(region) {
                (region, None, None) => {
                    candidate = region;
                }
                (region, Some(remainder), None) => {
                    map_builder.add_region(region);
                    candidate = remainder;
                }
                (region, Some(next), Some(remainder)) => {
                    map_builder.add_region(region);
                    map_builder.add_region(next);
                    candidate = remainder;
                }
                _ => unreachable!(),
            }
            current_frame = used_frames.next();
        }

        map_builder.add_region(candidate);
    }

    map_builder.build()
}

#[cfg(test)]
mod tests {
    use std::vec::Vec;

    use bootloader_api::info::{MemoryRegion, MemoryRegionKind};
    use x86_64::{
        structures::paging::{PhysFrame, Size4KiB},
        PhysAddr,
    };

    use crate::vmm;

    use super::build_memory_map;

    #[test]
    pub fn builds_accurate_memory_map() {
        let regions = std::vec![
            &MemoryRegion {
                start: 0x0000_0000,
                end: 0x0040_0000,
                kind: MemoryRegionKind::Usable,
            },
            &MemoryRegion {
                start: 0x0040_0000,
                end: 0x1000_0000,
                kind: MemoryRegionKind::Usable,
            },
            &MemoryRegion {
                start: 0x1000_0000,
                end: 0x2000_0000,
                kind: MemoryRegionKind::Bootloader,
            },
            &MemoryRegion {
                start: 0x2000_0000,
                end: 0x2000_1000,
                kind: MemoryRegionKind::Usable,
            },
            &MemoryRegion {
                start: 0x2000_1000,
                end: 0x4000_0000,
                kind: MemoryRegionKind::Usable,
            },
            &MemoryRegion {
                start: 0x4000_0000,
                end: 0x5000_0000,
                kind: MemoryRegionKind::Usable,
            },
        ];

        // Splatter some used frames in there
        let used_frames: Vec<PhysFrame<Size4KiB>> = std::vec![
            // Intentionally put these out of order.
            PhysFrame::containing_address(PhysAddr::new(0x0000_2000)),
            PhysFrame::containing_address(PhysAddr::new(0x0FFF_F000)), // On the trailing edge of the first region
            // Three contiguous frames
            PhysFrame::containing_address(PhysAddr::new(0x2000_0000)),
            PhysFrame::containing_address(PhysAddr::new(0x2000_1000)),
            PhysFrame::containing_address(PhysAddr::new(0x2000_2000)),
            PhysFrame::containing_address(PhysAddr::new(0x3000_3000)),
        ];

        let map = build_memory_map(regions.into_iter(), used_frames.into_iter());

        assert_eq!(
            &[
                vmm::MemoryRegion {
                    start: PhysAddr::new(0x0000_0000),
                    end: PhysAddr::new(0x0000_2000),
                    kind: vmm::MemoryRegionKind::Usable,
                },
                vmm::MemoryRegion {
                    start: PhysAddr::new(0x0000_2000),
                    end: PhysAddr::new(0x0000_3000),
                    kind: vmm::MemoryRegionKind::InUse(vmm::MemoryPurpose::KernelHeap)
                },
                vmm::MemoryRegion {
                    start: PhysAddr::new(0x0000_3000),
                    end: PhysAddr::new(0x0FFF_F000),
                    kind: vmm::MemoryRegionKind::Usable,
                },
                vmm::MemoryRegion {
                    start: PhysAddr::new(0x0FFF_F000),
                    end: PhysAddr::new(0x1000_0000),
                    kind: vmm::MemoryRegionKind::InUse(vmm::MemoryPurpose::KernelHeap),
                },
                vmm::MemoryRegion {
                    start: PhysAddr::new(0x1000_0000),
                    end: PhysAddr::new(0x2000_0000),
                    kind: vmm::MemoryRegionKind::Reserved(
                        vmm::ReservedMemoryKind::ReservedByBootloader
                    ),
                },
                vmm::MemoryRegion {
                    start: PhysAddr::new(0x2000_0000),
                    end: PhysAddr::new(0x2000_3000),
                    kind: vmm::MemoryRegionKind::InUse(vmm::MemoryPurpose::KernelHeap),
                },
                vmm::MemoryRegion {
                    start: PhysAddr::new(0x2000_3000),
                    end: PhysAddr::new(0x3000_3000),
                    kind: vmm::MemoryRegionKind::Usable,
                },
                vmm::MemoryRegion {
                    start: PhysAddr::new(0x3000_3000),
                    end: PhysAddr::new(0x3000_4000),
                    kind: vmm::MemoryRegionKind::InUse(vmm::MemoryPurpose::KernelHeap),
                },
                vmm::MemoryRegion {
                    start: PhysAddr::new(0x3000_4000),
                    end: PhysAddr::new(0x5000_0000),
                    kind: vmm::MemoryRegionKind::Usable,
                },
            ],
            map.regions()
        )
    }
}
