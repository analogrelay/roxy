use std::sync::Arc;

use crate::{
    Architecture, Error, PhysicalAddress, VirtualAddress,
    paging::{PageEntry, PageEntryAddress},
};

pub struct PageTable<'a, A> {
    base_address: VirtualAddress,
    addr: PhysicalAddress,
    level: usize,
    arch: &'a A,
}

impl<'a, A: Architecture> PageTable<'a, A> {
    pub unsafe fn new(
        base_address: VirtualAddress,
        start: PhysicalAddress,
        level: usize,
        arch: &'a A,
    ) -> Self {
        Self {
            base_address,
            addr: start,
            level,
            arch,
        }
    }

    /// Returns the base address for all entries in the table.
    ///
    /// This is the virtual address represented by the first entry in the table.
    pub fn base_address(&self) -> VirtualAddress {
        self.base_address
    }

    /// Gets the physical address of the table.
    pub fn address(&self) -> PhysicalAddress {
        self.addr
    }

    /// Gets the virtual address of the table.
    pub fn virtual_address(&self) -> VirtualAddress {
        unsafe { self.arch.virtual_address_for(self.addr) }
    }

    /// Gets the level of the table.
    pub fn level(&self) -> usize {
        self.level
    }

    /// Gets the virtual address of the entry at the given index.
    pub fn entry_virtual_address(&self, i: usize) -> Option<VirtualAddress> {
        let phys = if i < A::PAGE_ENTRIES {
            Some(self.addr + i * A::PAGE_ENTRY_SIZE)
        } else {
            None
        };
        phys.map(|p| self.arch.virtual_address_for(p))
    }

    /// Gets the entry at the given index.
    ///
    /// # Safety
    ///
    /// There is no guarantee that the entry is valid.
    pub unsafe fn entry(&self, i: usize) -> Result<PageEntry<A>, Error> {
        self.entry_virtual_address(i)
            .map(|a| unsafe { PageEntry::from_value(self.arch.read::<usize>(a)) })
            .ok_or_else(|| Error::PageTableIndexOutOfRange(i))
    }

    /// Sets the entry at the given index.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the entry is valid.
    pub unsafe fn set_entry(&mut self, i: usize, entry: PageEntry<A>) -> Result<(), Error> {
        let Some(addr) = self.entry_virtual_address(i) else {
            return Err(Error::PageTableIndexOutOfRange(i));
        };
        unsafe {
            self.arch.write(addr, entry.value());
        }
        Ok(())
    }

    /// Gets the index of the entry that maps the given address.
    pub fn index_of(&self, address: VirtualAddress) -> Option<usize> {
        // Canonicalize the address
        let address = VirtualAddress::new(address.value() & A::PAGE_ADDRESS_MASK);

        // How far do we need to shift it to get the entry index matching this level?
        let shift = self.level * A::PAGE_ENTRY_SHIFT + A::PAGE_SHIFT;

        let level_max = A::PAGE_ENTRIES.wrapping_shl(shift as u32).wrapping_sub(1);
        if address >= self.base_address && address <= self.base_address + level_max {
            Some((address.value() - self.base_address.value()) >> shift)
        } else {
            None
        }
    }

    /// Gets the next table at the given index.
    ///
    /// Returns `Err(Error::PageTableIsLeaf)` if the table is a leaf table.
    /// Returns `Err(Error::PageTableIndexOutOfRange)` if the index is out of range.
    /// Returns `Ok(None)` if the entry is not present.
    /// Returns `Ok(Some(PageTable))` if the entry is present.
    pub unsafe fn next_table(&self, i: usize) -> Result<Option<PageTable<'a, A>>, Error> {
        if self.level == 0 {
            return Err(Error::PageTableIsLeaf);
        }

        let entry_base = if i < A::PAGE_ENTRIES {
            self.base_address + i * A::PAGE_SIZE
        } else {
            return Err(Error::PageTableIndexOutOfRange(i));
        };

        // SAFETY: We have to assume that the entry is valid, otherwise it wouldn't be present.
        let entry = unsafe { self.entry(i)? };

        if let Ok(addr) = entry.address() {
            Ok(Some(unsafe {
                PageTable::new(entry_base, addr, self.level - 1, self.arch)
            }))
        } else {
            Ok(None)
        }
    }
}
