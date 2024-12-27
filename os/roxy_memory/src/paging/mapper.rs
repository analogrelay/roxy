use std::sync::Arc;

use crate::{
    Architecture, Error, PhysicalAddress, VirtualAddress,
    allocator::FrameAllocator,
    paging::{PageEntry, TableKind},
};

use super::{PageFlags, PageTable};

pub struct FlushPromise<'a, A: Architecture> {
    address: VirtualAddress,
    arch: &'a A,
}

impl<'a, A: Architecture> FlushPromise<'a, A> {
    pub fn new(address: VirtualAddress, arch: &'a A) -> Self {
        Self { address, arch }
    }

    pub fn flush(self) {
        self.arch.invalidate_one(self.address);
    }
}

/// A page mapper is a structure to manage an entire page table hierarchy.
pub struct PageMapper<'a, A, F> {
    table_kind: TableKind,
    table_address: PhysicalAddress,
    allocator: F,
    arch: &'a A,
}

impl<'a, A: Architecture, F: FrameAllocator> PageMapper<'a, A, F> {
    /// Creates a new `PageMapper` with the given table kind, table address, allocator, and architecture.
    ///
    /// # Safety
    ///
    /// The table address must be a valid physical address containing the root page table being managed.
    pub unsafe fn new(
        table_kind: TableKind,
        table_address: PhysicalAddress,
        allocator: F,
        arch: &'a A,
    ) -> Self {
        Self {
            table_kind,
            table_address,
            allocator,
            arch,
        }
    }

    /// Creates a brand new page table hierarchy in an available frame.
    pub unsafe fn create(table_kind: TableKind, mut allocator: F, arch: &'a A) -> Option<Self> {
        let table_address = unsafe { allocator.allocate_frame(1)? };
        Some(unsafe { Self::new(table_kind, table_address.start, allocator, arch) })
    }

    /// Loads the active page table hierarchy from architecture-defined registers.
    pub unsafe fn from_active(table_kind: TableKind, allocator: F, arch: &'a A) -> Self {
        let table_address = arch.page_table_address(table_kind);
        unsafe { Self::new(table_kind, table_address, allocator, arch) }
    }

    /// Checks if the current page table is the active one.
    pub fn active(&self) -> bool {
        self.arch.page_table_address(self.table_kind) == self.table_address
    }

    /// Makes this page table hierarchy the active one.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the page table hierarchy is valid and safe to use.
    pub unsafe fn activate(&self) {
        unsafe {
            // SAFETY: The caller is assuming all safety guarantees.
            self.arch
                .set_page_table_address(self.table_kind, self.table_address);
        }
    }

    /// Gets the root page table of the hierarchy.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the root table is valid.
    pub fn root_table(&self) -> PageTable<'a, A> {
        unsafe {
            // SAFETY: The caller is responsible for ensuring that the table is valid.
            PageTable::new(
                VirtualAddress::new(0),
                self.table_address,
                A::PAGE_LEVELS - 1,
                self.arch,
            )
        }
    }

    /// Maps the provided range of virtual addresses to the provided range of physical addresses.
    pub unsafe fn map_to(
        &mut self,
        virtual_address: VirtualAddress,
        physical_address: PhysicalAddress,
        flags: PageFlags<A>,
    ) -> Result<FlushPromise<A>, Error> {
        // Walk down page tables, creating as we go, until we reach the level that should be mapped.
        let mut leaf_table = self.walk_to_leaf(virtual_address, PageFlags::new_for_table())?;
        assert_eq!(0, leaf_table.level());

        // Map the range in the leaf table.
        let index = A::index_at_level(virtual_address, 0);
        unsafe { leaf_table.set_entry(index, PageEntry::new(physical_address, flags))? };
        Ok(FlushPromise::new(virtual_address, self.arch))
    }

    /// Walks down the page table hierarchy to the leaf table that should contain the provided virtual address.
    /// Creates any missing tables along the way.
    fn walk_to_leaf(
        &mut self,
        virtual_address: VirtualAddress,
        table_flags: PageFlags<A>,
    ) -> Result<PageTable<'a, A>, Error> {
        let mut table = self.root_table();
        for level in (1..A::PAGE_LEVELS).rev() {
            let index = A::index_at_level(virtual_address, level);
            table = match unsafe { table.next_table(index) }? {
                Some(t) => t,
                None => {
                    // Allocate a new table
                    let frame = unsafe { self.allocator.allocate_one() };
                    let frame = frame.ok_or(Error::OutOfPhysicalMemory)?;
                    let new_table = unsafe {
                        PageTable::new(
                            A::base_at_level(virtual_address, level),
                            frame,
                            level - 1,
                            self.arch,
                        )
                    };
                    let entry = PageEntry::new(frame, table_flags);
                    unsafe { table.set_entry(index, entry).unwrap() };
                    new_table
                }
            };
        }

        Ok(table)
    }
}

#[cfg(test)]
mod test {
    use crate::{
        Architecture, Emulated, EmulatedMachine, VirtualAddress, X8664,
        allocator::{BumpFrameAllocator, FrameAllocator},
        paging::{PageFlags, PageMapper, TableKind},
    };

    #[test]
    pub fn map_physical_address() {
        const TEST_VADDR: VirtualAddress = VirtualAddress::new(0x40000);
        const TEST_VALUE: u32 = 0x12345678;
        let (machine, areas) = EmulatedMachine::<X8664>::new(64 * 1024 * 1024);
        let arch = Emulated::new(X8664, machine);
        let mut allocator = BumpFrameAllocator::new(&arch, &areas);

        // Get a frame to map
        let frame = unsafe { allocator.allocate_one().unwrap() };

        let mut mapper = unsafe {
            PageMapper::new(
                TableKind::Kernel,
                arch.machine().page_table_address(TableKind::Kernel),
                allocator,
                &arch,
            )
        };

        unsafe {
            mapper
                .map_to(TEST_VADDR, frame, PageFlags::new().with_writable(true))
                .unwrap()
                .flush()
        };

        // Now, write to the mapped address
        unsafe {
            arch.write(TEST_VADDR, TEST_VALUE);
        }

        // Try to read from the physical address and see if the value is the same.
        let read_value: u32 = arch.machine().read_physical(frame).unwrap();
        assert_eq!(TEST_VALUE, read_value);

        // Try to read from the virtual address and see if the value is the same.
        let read_value: u32 = unsafe { arch.read(TEST_VADDR) };
        assert_eq!(TEST_VALUE, read_value);
    }
}
