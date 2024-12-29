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
    pub unsafe fn create(
        table_kind: TableKind,
        mut allocator: F,
        arch: &'a A,
    ) -> Result<Self, Error> {
        let table_address = unsafe { allocator.allocate_frame(1)? };
        Ok(unsafe { Self::new(table_kind, table_address.start, allocator, arch) })
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

    /// Maps the provided virtual address to a new frame.
    pub unsafe fn map(
        &mut self,
        virtual_address: VirtualAddress,
        flags: PageFlags<A>,
    ) -> Result<FlushPromise<A>, Error> {
        let phys = unsafe { self.allocator.allocate_one()? };
        unsafe { self.map_to(virtual_address, phys, flags) }
    }

    /// Maps the provided range of virtual addresses to the provided range of physical addresses.
    pub unsafe fn map_to(
        &mut self,
        virtual_address: VirtualAddress,
        physical_address: PhysicalAddress,
        flags: PageFlags<A>,
    ) -> Result<FlushPromise<A>, Error> {
        // Validate the addresses and flags
        if virtual_address % A::PAGE_SIZE != 0 {
            return Err(Error::NotPageAligned(virtual_address.value()));
        }
        if physical_address % A::PAGE_SIZE != 0 {
            return Err(Error::NotPageAligned(physical_address.value()));
        }
        if !flags.validate() {
            return Err(Error::InvalidPageFlags(flags.value()));
        }

        // Walk down page tables, creating as we go, until we reach the level that should be mapped.
        let mut leaf_table = self.walk_to_leaf(virtual_address, PageFlags::new_for_table())?;
        assert_eq!(0, leaf_table.level());

        // Map the range in the leaf table.
        let index = A::index_at_level(virtual_address, 0);
        unsafe { leaf_table.set_entry(index, PageEntry::new(physical_address, flags))? };
        Ok(FlushPromise::new(virtual_address, self.arch))
    }

    pub unsafe fn remap_entry_with(
        &mut self,
        virtual_address: VirtualAddress,
        remap: impl FnOnce(PageEntry<A>) -> PageEntry<A>,
    ) -> Result<FlushPromise<A>, Error> {
        let (mut table, index) = self
            .find_leaf_entry(virtual_address)?
            .ok_or(Error::PageNotMapped)?;
        unsafe { table.set_entry(index, remap(table.entry(index)?)) }?;
        Ok(FlushPromise::new(virtual_address, self.arch))
    }

    pub unsafe fn remap_with(
        &mut self,
        virtual_address: VirtualAddress,
        remap: impl FnOnce(PageFlags<A>) -> PageFlags<A>,
    ) -> Result<FlushPromise<A>, Error> {
        unsafe {
            self.remap_entry_with(virtual_address, |mut e| {
                let flags = e.flags();
                let new_flags = remap(flags);
                e.set_flags(new_flags);
                e
            })
        }
    }

    pub unsafe fn remap(
        &mut self,
        virtual_address: VirtualAddress,
        new_flags: PageFlags<A>,
    ) -> Result<FlushPromise<A>, Error> {
        unsafe { self.remap_with(virtual_address, |_| new_flags) }
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
                    let frame = unsafe { self.allocator.allocate_one()? };
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

    fn find_leaf_entry(
        &self,
        virtual_address: VirtualAddress,
    ) -> Result<Option<(PageTable<A>, usize)>, Error> {
        let mut table = self.root_table();
        for level in (1..A::PAGE_LEVELS).rev() {
            let index = A::index_at_level(virtual_address, level);
            table = match unsafe { table.next_table(index) }? {
                Some(t) => t,
                None => return Ok(None),
            };
        }

        Ok(Some((table, A::index_at_level(virtual_address, 0))))
    }
}

#[cfg(test)]
mod test {
    use crate::{
        Emulated, EmulatedMachine, Error, VirtualAddress, X8664,
        allocator::{BumpFrameAllocator, FrameAllocator},
        paging::{PageEntry, PageFlags, PageMapper, TableKind},
    };

    #[test]
    pub fn map() {
        const TEST_VADDR: VirtualAddress = VirtualAddress::new(0x60000);
        const TEST_VALUE: u32 = 0x12345678;
        let (machine, areas) = EmulatedMachine::<X8664>::new(64 * 1024 * 1024);
        let arch = Emulated::new(X8664, machine);
        let allocator = BumpFrameAllocator::new(&arch, &areas);

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
                .map(TEST_VADDR, PageFlags::new().with_writable(true))
                .unwrap()
                .flush()
        };

        // Now, write to the mapped address
        unsafe {
            arch.try_write(TEST_VADDR, TEST_VALUE).unwrap();
        }

        // Try to read from the virtual address and see if the value is the same.
        let read_value: u32 = unsafe { arch.try_read(TEST_VADDR).unwrap() };
        assert_eq!(TEST_VALUE, read_value);
    }

    #[test]
    pub fn map_to() {
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
            arch.try_write(TEST_VADDR, TEST_VALUE).unwrap();
        }

        // Try to read from the physical address and see if the value is the same.
        let read_value: u32 = arch.machine().read_physical(frame).unwrap();
        assert_eq!(TEST_VALUE, read_value);

        // Try to read from the virtual address and see if the value is the same.
        let read_value: u32 = unsafe { arch.try_read(TEST_VADDR).unwrap() };
        assert_eq!(TEST_VALUE, read_value);
    }

    #[test]
    pub fn remap() {
        const TEST_VADDR: VirtualAddress = VirtualAddress::new(0x40000);
        const TEST_VALUE: u32 = 0x12345678;
        let (machine, areas) = EmulatedMachine::<X8664>::new(64 * 1024 * 1024);
        let arch = Emulated::new(X8664, machine);
        let mut allocator = BumpFrameAllocator::new(&arch, &areas);

        // Get a frame to map
        let frame_1 = unsafe { allocator.allocate_one().unwrap() };
        let frame_2 = unsafe { allocator.allocate_one().unwrap() };

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
                .map_to(TEST_VADDR, frame_1, PageFlags::new().with_writable(true))
                .unwrap()
                .flush()
        };

        // Now, write to the mapped address
        unsafe {
            arch.try_write(TEST_VADDR, TEST_VALUE).unwrap();
        }

        let read_value: u32 = arch.machine().read_physical(frame_1).unwrap();
        assert_eq!(TEST_VALUE, read_value);

        // Now, remap the same address to be read-only and try to write to it.
        unsafe {
            mapper
                .remap(TEST_VADDR, PageFlags::new().with_writable(false))
                .unwrap()
                .flush()
        };
        let r = unsafe { arch.try_write(TEST_VADDR, TEST_VALUE + 1) };
        assert_eq!(Error::PageIsReadOnly, r.unwrap_err());

        // The panic poisoned the lock, so we need to reset it.
        arch.clear_poison();

        // Try remapping as writable, in a new location
        unsafe {
            mapper
                .remap_entry_with(TEST_VADDR, |e| {
                    PageEntry::new(frame_2, e.flags().with_writable(true))
                })
                .unwrap()
                .flush()
        };

        // Now, write to the new mapped address
        unsafe {
            arch.try_write(TEST_VADDR, TEST_VALUE + 2).unwrap();
        }

        // Read from the old physical address and see if the value is the same.
        assert_eq!(TEST_VALUE, arch.machine().read_physical(frame_1).unwrap());

        // And from the new physical address, which should have the new value.
        assert_eq!(
            TEST_VALUE + 2,
            arch.machine().read_physical(frame_2).unwrap()
        );
    }
}
