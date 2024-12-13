use alloc::vec::Vec;
use x86_64::PhysAddr;

pub enum ReservedMemoryKind {
    ReservedByBootloader,
    ReservedByUefi(u32),
    ReservedByBios(u32),
}

pub enum MemoryRegionKind {
    Usable,
    KernelHeap,
    Reserved(ReservedMemoryKind),
}

pub struct MemoryRegion {
    pub start: PhysAddr,
    pub end: PhysAddr,
    pub kind: MemoryRegionKind,
}

/// Contains a full map of Physical Memory
pub struct MemoryMap {
    regions: Vec<MemoryRegion>,
}

impl MemoryMap {
    pub fn builder() -> MemoryMapBuilder {
        MemoryMapBuilder(Self {
            regions: Vec::new(),
        })
    }
}

pub struct MemoryMapBuilder(MemoryMap);

impl MemoryMapBuilder {
    pub fn add_region(&mut self, start: PhysAddr, end: PhysAddr, kind: MemoryRegionKind) {
        self.0.regions.push(MemoryRegion { start, end, kind })
    }

    pub fn build(self) -> MemoryMap {
        self.0
    }
}
