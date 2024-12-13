use alloc::{boxed::Box, vec::Vec};
use x86_64::PhysAddr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReservedMemoryKind {
    Unknown,
    ReservedByBootloader,
    ReservedByUefi(u32),
    ReservedByBios(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryRegionKind {
    Usable,
    InUse(MemoryPurpose),
    Reserved(ReservedMemoryKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryPurpose {
    Unknown,
    KernelHeap,
    KernelPageTables,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryRegion {
    pub start: PhysAddr,
    pub end: PhysAddr,
    pub kind: MemoryRegionKind,
}

impl MemoryRegion {
    pub fn new(start: PhysAddr, end: PhysAddr, kind: MemoryRegionKind) -> Self {
        Self { start, end, kind }
    }

    pub fn size(&self) -> u64 {
        self.end.as_u64() - self.start.as_u64()
    }

    // This could probably be reconciled with `try_merge` to avoid duplication.
    pub fn try_append(&mut self, other: &MemoryRegion) -> bool {
        if other.start == self.end && other.kind == self.kind {
            self.end = other.end;
            true
        } else {
            false
        }
    }

    /// Try to merge two memory regions.
    ///
    /// There are three possible outcomes:
    /// 1. The regions do not overlap, and cannot be merged.
    /// 2. The regions overlap, or are adjacent, and can be merged.
    /// 3. The regions overlap, but are not the same kind, and the containing region must be split.
    ///
    /// The result is a tuple representing:
    /// 1. The new state of the "current" region.
    /// 2. The "next" complete region, if there is one.
    /// 3. The "remaining" segment after the "next" region, if there is one.
    ///
    /// Basically, all regions **up to** but excluding the last `Some` value in the tuple can be immediately emitted.
    /// The last one should be buffered and become the next "current" region.
    pub fn try_merge(
        self,
        other: MemoryRegion,
    ) -> (MemoryRegion, Option<MemoryRegion>, Option<MemoryRegion>) {
        if other.start > self.end {
            // No overlap
            (self, Some(other), None)
        } else if other.kind == self.kind {
            // They overlap, or are adjacent, and can be merged.
            // We don't really care which scenario we're in, as long as the kinds are the same.
            (
                MemoryRegion::new(self.start, self.end.max(other.end), self.kind),
                None,
                None,
            )
        } else if other.start == self.end {
            // They're adjacent, but not the same kind.
            (self, Some(other), None)
        } else {
            // We need to split.
            if self.start < other.start {
                (
                    MemoryRegion::new(self.start, other.start, self.kind),
                    Some(MemoryRegion::new(other.start, other.end, other.kind)),
                    if other.end < self.end {
                        Some(MemoryRegion::new(other.end, self.end, self.kind))
                    } else {
                        None
                    },
                )
            } else {
                (
                    MemoryRegion::new(other.start, other.end, other.kind),
                    if other.end < self.end {
                        Some(MemoryRegion::new(other.end, self.end, self.kind))
                    } else {
                        None
                    },
                    None,
                )
            }
        }
    }
}

/// Contains a full map of Physical Memory
pub struct MemoryMap {
    regions: Box<[MemoryRegion]>,
    total_memory: u64,
    usable_memory: u64,
}

impl MemoryMap {
    pub fn builder() -> MemoryMapBuilder {
        MemoryMapBuilder(Vec::new())
    }

    fn new(regions: Box<[MemoryRegion]>) -> Self {
        let total_memory = regions.iter().map(|r| r.size()).sum();
        let usable_memory = regions
            .iter()
            .filter(|r| !matches!(r.kind, MemoryRegionKind::Reserved(_)))
            .map(|r| r.size())
            .sum();
        Self {
            regions,
            total_memory,
            usable_memory,
        }
    }

    pub fn regions(&self) -> &[MemoryRegion] {
        &self.regions
    }

    pub fn total_memory(&self) -> u64 {
        self.total_memory
    }

    pub fn usable_memory(&self) -> u64 {
        self.usable_memory
    }

    pub fn reserved_memory(&self) -> u64 {
        self.total_memory - self.usable_memory
    }
}

pub struct MemoryMapBuilder(Vec<MemoryRegion>);

impl MemoryMapBuilder {
    pub fn add_region(&mut self, region: MemoryRegion) {
        // Try to merge this region into the previous one.
        match self.0.last_mut() {
            Some(prev) => {
                if !prev.try_append(&region) {
                    // We failed, so just add this to the end
                    self.0.push(region);
                }
            }
            None => {
                // No previous region, so just add this to the end
                self.0.push(region);
            }
        }
    }

    pub fn build(self) -> MemoryMap {
        MemoryMap::new(self.0.into_boxed_slice())
    }
}

#[cfg(test)]
mod test {
    use x86_64::PhysAddr;

    use crate::vmm::{MemoryPurpose, MemoryRegion, MemoryRegionKind};

    #[test]
    pub fn try_merge_non_overlapping_or_adjacent() {
        let left = MemoryRegion::new(
            PhysAddr::new(0x0000),
            PhysAddr::new(0x1000),
            MemoryRegionKind::Usable,
        );
        let right = MemoryRegion::new(
            PhysAddr::new(0x2000),
            PhysAddr::new(0x3000),
            MemoryRegionKind::Usable,
        );
        assert_eq!(
            (left.clone(), Some(right.clone()), None),
            left.try_merge(right),
        );
    }

    #[test]
    pub fn try_merge_adjacent_same_kind() {
        let left = MemoryRegion::new(
            PhysAddr::new(0x0000),
            PhysAddr::new(0x1000),
            MemoryRegionKind::Usable,
        );
        let right = MemoryRegion::new(
            PhysAddr::new(0x1000),
            PhysAddr::new(0x2000),
            MemoryRegionKind::Usable,
        );
        assert_eq!(
            (
                MemoryRegion::new(left.start, right.end, left.kind),
                None,
                None
            ),
            left.try_merge(right),
        );
    }

    #[test]
    pub fn try_merge_adjacent_different_kind() {
        let left = MemoryRegion::new(
            PhysAddr::new(0x0000),
            PhysAddr::new(0x1000),
            MemoryRegionKind::Usable,
        );
        let right = MemoryRegion::new(
            PhysAddr::new(0x1000),
            PhysAddr::new(0x2000),
            MemoryRegionKind::InUse(MemoryPurpose::KernelHeap),
        );
        assert_eq!(
            (left.clone(), Some(right.clone()), None),
            left.try_merge(right),
        );
    }

    #[test]
    pub fn try_merge_split_overlap_at_start() {
        let left = MemoryRegion::new(
            PhysAddr::new(0x0000),
            PhysAddr::new(0x2000),
            MemoryRegionKind::Usable,
        );
        let right = MemoryRegion::new(
            PhysAddr::new(0x0000),
            PhysAddr::new(0x1000),
            MemoryRegionKind::InUse(MemoryPurpose::KernelHeap),
        );
        assert_eq!(
            (
                right.clone(),
                Some(MemoryRegion::new(right.end, left.end, left.kind)),
                None
            ),
            left.try_merge(right),
        );
    }

    #[test]
    pub fn try_merge_split_overlap_at_end() {
        let left = MemoryRegion::new(
            PhysAddr::new(0x0000),
            PhysAddr::new(0x2000),
            MemoryRegionKind::Usable,
        );
        let right = MemoryRegion::new(
            PhysAddr::new(0x1000),
            PhysAddr::new(0x2000),
            MemoryRegionKind::InUse(MemoryPurpose::KernelHeap),
        );
        assert_eq!(
            (
                MemoryRegion::new(left.start, right.start, left.kind),
                Some(right.clone()),
                None
            ),
            left.try_merge(right),
        );
    }

    #[test]
    pub fn try_merge_split_overlap_in_middle() {
        let left = MemoryRegion::new(
            PhysAddr::new(0x0000),
            PhysAddr::new(0x4000),
            MemoryRegionKind::Usable,
        );
        let right = MemoryRegion::new(
            PhysAddr::new(0x1000),
            PhysAddr::new(0x2000),
            MemoryRegionKind::InUse(MemoryPurpose::KernelHeap),
        );
        assert_eq!(
            (
                MemoryRegion::new(left.start, right.start, left.kind),
                Some(MemoryRegion::new(right.start, right.end, right.kind)),
                Some(MemoryRegion::new(right.end, left.end, left.kind)),
            ),
            left.try_merge(right),
        );
    }

    #[test]
    pub fn try_merge_absorb_overlap() {
        let left = MemoryRegion::new(
            PhysAddr::new(0x0000),
            PhysAddr::new(0x4000),
            MemoryRegionKind::Usable,
        );
        let right = MemoryRegion::new(
            PhysAddr::new(0x1000),
            PhysAddr::new(0x2000),
            MemoryRegionKind::Usable,
        );
        assert_eq!((left.clone(), None, None,), left.try_merge(right),);
    }
}
