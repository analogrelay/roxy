use core::ops::Range;

use crate::{Architecture, PhysicalAddress, UsableMemoryRegion, allocator::FrameAllocator};

use super::FrameUsage;

pub struct BumpFrameAllocator<'a, A> {
    arch: &'a A,

    /// The original set of regions that were passed to the allocator.
    start_regions: &'a [UsableMemoryRegion],

    /// Stores the current set of usable regions,
    /// as well as the offset of the first free address in the first region.
    ///
    /// The region `current_regions.0[0][0..current_regions.1]` is IN USE.
    current_regions: (&'a [UsableMemoryRegion], usize),
}

impl<'a, A: Architecture> BumpFrameAllocator<'a, A> {
    pub fn new(arch: &'a A, memory_regions: &'a [UsableMemoryRegion]) -> Self {
        BumpFrameAllocator {
            arch,
            start_regions: memory_regions,
            current_regions: (memory_regions, 0),
        }
    }
}

impl<'a, A: Architecture> FrameAllocator for BumpFrameAllocator<'a, A> {
    unsafe fn allocate_frame(&mut self, count: usize) -> Option<Range<PhysicalAddress>> {
        let size = count * A::PAGE_SIZE;

        loop {
            let area = self.current_regions.0.first()?;
            let offset = self.current_regions.1;
            if area.size - offset < size {
                // There isn't enough space in this region, so we move to the next one.
                self.current_regions = (&self.current_regions.0[1..], 0);
                continue;
            }

            // Reserve the space in the region.
            let start = area.base + offset;
            self.current_regions.1 += size;

            // Clear the region
            unsafe {
                // SAFETY: We reserved this region, so it's safe to write to it.
                self.arch
                    .write_bytes(self.arch.virtual_address_for(start), 0, size);
            }
            return Some(start..(start + size));
        }
    }

    unsafe fn free(&mut self, _address: PhysicalAddress, _count: usize) {
        unimplemented!("bump allocator cannot free frames");
    }

    unsafe fn usage(&self) -> FrameUsage {
        let total = self.start_regions.iter().map(|a| a.size).sum::<usize>();
        let free =
            self.current_regions.0.iter().map(|a| a.size).sum::<usize>() - self.current_regions.1;
        let used = (total - free) / A::PAGE_SIZE;
        let total = total / A::PAGE_SIZE;
        FrameUsage { total, used }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{Emulated, EmulatedMachine, X8664};

    #[test]
    fn basic_bump_allocation() {
        let (machine, areas) = EmulatedMachine::new(64 * 1024 * 1024);
        let free_start = areas[0].base;
        let arch = Emulated::new(X8664, machine);
        let mut allocator = BumpFrameAllocator::new(&arch, &areas);

        let region = unsafe { allocator.allocate_frame(2) };
        assert_eq!(Some(free_start..free_start + 2 * X8664::PAGE_SIZE), region);
    }

    #[test]
    fn bump_allocation_with_unusable_holes() {
        let (machine, areas) = EmulatedMachine::new(64 * 1024 * 1024);
        let free_start = areas[0].base;

        // We have only 6 frames of usable memory, and there are 3 holes of 2 frames each.
        let areas = vec![
            // 0..2
            UsableMemoryRegion {
                base: free_start,
                size: 2 * X8664::PAGE_SIZE,
            },
            // 4..6
            UsableMemoryRegion {
                base: free_start + 4 * X8664::PAGE_SIZE,
                size: 2 * X8664::PAGE_SIZE,
            },
            // 8..10
            UsableMemoryRegion {
                base: free_start + 8 * X8664::PAGE_SIZE,
                size: 2 * X8664::PAGE_SIZE,
            },
        ];
        let arch = Emulated::new(X8664, machine);
        let mut allocator = BumpFrameAllocator::new(&arch, &areas);

        // Try to allocate in the first hole
        let region = unsafe { allocator.allocate_frame(1) };
        assert_eq!(Some(free_start..free_start + X8664::PAGE_SIZE), region);

        // Now, try to allocate 2 frames, which should jump us to the second hole
        let region = unsafe { allocator.allocate_frame(2) };
        assert_eq!(
            Some(free_start + 4 * X8664::PAGE_SIZE..free_start + 6 * X8664::PAGE_SIZE),
            region
        );

        // Then, try to allocate 3 frames, which should fail, because we're at the end of usable memory;
        let region = unsafe { allocator.allocate_frame(3) };
        assert_eq!(None, region);
    }
}
