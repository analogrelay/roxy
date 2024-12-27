use std::collections::BTreeMap;

use crate::{
    Architecture, Error, PhysicalAddress, UsableMemoryRegion, VirtualAddress,
    paging::{PageEntry, PageFlags, TableKind},
};

use super::Emulated;

pub struct EmulatedMachine<A> {
    /// Represents the physical RAM of the emulated machine.
    memory: Box<[u8]>,

    /// Represents the "Root Page Table" register (CR3 in x86-64, for example).
    kernel_page_table_address: PhysicalAddress,

    /// Represents the "User Page Table" register (no equivalent in x86-64).
    user_page_table_address: PhysicalAddress,

    /// Represents the Translation Lookaside Buffer (TLB) of the emulated machine.
    tlb: BTreeMap<VirtualAddress, PageEntry<Emulated<A>>>,
}

impl<A: Architecture> EmulatedMachine<A> {
    pub fn new(size: usize) -> (Self, Vec<UsableMemoryRegion>) {
        let (memory, root_table, usable_regions) = MemoryBuilder::<A>::build(size);

        (
            Self {
                // Use "0xBE" as a default value for memory, so we can tell if memory has been initialized.
                memory,
                kernel_page_table_address: root_table,
                user_page_table_address: root_table,
                tlb: BTreeMap::new(),
            },
            usable_regions,
        )
    }

    pub fn read<T>(&mut self, address: VirtualAddress) -> Result<T, Error> {
        let mem = self.get_memory(address, std::mem::size_of::<T>())?;
        Ok(unsafe { std::ptr::read(mem as *const T) })
    }

    pub fn write<T>(&mut self, address: VirtualAddress, value: T) -> Result<(), Error> {
        let mem = self.get_memory_mut(address, std::mem::size_of::<T>())?;
        unsafe {
            std::ptr::write(mem as *mut T, value);
        }
        Ok(())
    }

    pub fn write_bytes(
        &mut self,
        address: VirtualAddress,
        value: u8,
        count: usize,
    ) -> Result<(), Error> {
        let mem = self.get_memory_mut(address, count)?;
        unsafe {
            std::ptr::write_bytes(mem, value, count);
        }
        Ok(())
    }

    /// Reads a value directly from the physical memory of the emulated machine.
    pub fn read_physical<T>(&self, address: PhysicalAddress) -> Result<T, Error> {
        let mem = self.get_physical_memory(address, std::mem::size_of::<T>())?;
        Ok(unsafe { std::ptr::read(mem as *const T) })
    }

    /// Writes a value directly to the physical memory of the emulated machine.
    pub fn write_physical<T>(&mut self, address: PhysicalAddress, value: T) -> Result<(), Error> {
        let mem = self.get_physical_memory_mut(address, std::mem::size_of::<T>())?;
        unsafe {
            std::ptr::write(mem as *mut T, value);
        }
        Ok(())
    }

    fn resolve_address(
        &mut self,
        address: VirtualAddress,
        writable: bool,
    ) -> Result<PhysicalAddress, Error> {
        if let Some(entry) = self.translate(address)? {
            let Ok(addr) = entry.address() else {
                return Err(Error::PageNotMapped);
            };
            if writable && !entry.flags().writable() {
                return Err(Error::PageIsReadOnly);
            }
            let offset = address.value() & A::PAGE_OFFSET_MASK;
            Ok(addr + offset)
        } else {
            Err(Error::PageNotMapped)
        }
    }

    fn get_memory(&mut self, address: VirtualAddress, size: usize) -> Result<*const u8, Error> {
        let addr = self.resolve_address(address, false)?;
        self.get_physical_memory(addr, size)
    }

    fn get_memory_mut(&mut self, address: VirtualAddress, size: usize) -> Result<*mut u8, Error> {
        let addr = self.resolve_address(address, true)?;
        self.get_physical_memory_mut(addr, size)
    }

    fn get_physical_memory(
        &self,
        address: PhysicalAddress,
        size: usize,
    ) -> Result<*const u8, Error> {
        let offset = address.value();
        if offset + size > self.memory.len() {
            Err(Error::PhysicalAddressOutOfRange)
        } else {
            Ok(self.memory[offset..(offset + size)].as_ptr())
        }
    }

    fn get_physical_memory_mut(
        &mut self,
        address: PhysicalAddress,
        size: usize,
    ) -> Result<*mut u8, Error> {
        let offset = address.value();
        if offset + size > self.memory.len() {
            Err(Error::PhysicalAddressOutOfRange)
        } else {
            Ok(self.memory[offset..(offset + size)].as_mut_ptr())
        }
    }

    fn translate(
        &mut self,
        address: VirtualAddress,
    ) -> Result<Option<PageEntry<Emulated<A>>>, Error> {
        // Try the TLB first
        if let Some(entry) = self.tlb.get(&address).cloned() {
            Ok(Some(entry))
        } else {
            if let Some(entry) =
                self.translate_using_page_table(address, self.kernel_page_table_address)?
            {
                self.tlb.insert(address, entry);
                Ok(Some(entry))
            } else {
                Ok(None)
            }
        }
    }

    pub fn page_table_address(&self, table_kind: crate::paging::TableKind) -> PhysicalAddress {
        match table_kind {
            TableKind::Kernel => self.kernel_page_table_address,
            TableKind::User => self.user_page_table_address,
        }
    }

    pub fn set_page_table_address(
        &mut self,
        table_kind: crate::paging::TableKind,
        address: PhysicalAddress,
    ) {
        match table_kind {
            TableKind::Kernel => self.kernel_page_table_address = address,
            TableKind::User => self.user_page_table_address = address,
        }
    }

    pub fn invalidate_one(&mut self, addr: VirtualAddress) {
        self.tlb.remove(&addr);
    }

    pub fn invalidate_all(&mut self, _table_kind: TableKind) {
        self.tlb.clear();
    }

    fn translate_using_page_table(
        &mut self,
        address: VirtualAddress,
        kernel_page_table_address: PhysicalAddress,
    ) -> Result<Option<PageEntry<Emulated<A>>>, Error> {
        let mut current_entry = PageEntry::new(kernel_page_table_address, PageFlags::new());
        for level in (0..A::PAGE_LEVELS).rev() {
            let index = A::index_at_level(address, level);
            let start = current_entry.address().unwrap() + index * A::PAGE_ENTRY_SIZE;
            let entry: usize = self.read_physical(start)?;
            let entry = unsafe { crate::paging::PageEntry::<Emulated<A>>::from_value(entry) };

            current_entry = match entry.address() {
                Ok(_) => entry,
                Err(_) => return Ok(None),
            };
        }
        Ok(Some(current_entry))
    }
}

struct MemoryBuilder<A> {
    memory: Box<[u8]>,
    frames_allocated: usize,
    root_table: PhysicalAddress,
    phantom: std::marker::PhantomData<A>,
}

impl<A: Architecture> MemoryBuilder<A> {
    pub fn build(size: usize) -> (Box<[u8]>, PhysicalAddress, Vec<UsableMemoryRegion>) {
        assert!(
            size >= A::PAGE_SIZE,
            "memory size must be at least one page"
        );
        assert!(
            size % A::PAGE_SIZE == 0,
            "memory size must be a multiple of the page size"
        );

        let mut memory = vec![0xBEu8; size].into_boxed_slice();

        // Create the root page table.
        let root_table = PhysicalAddress::new(0);
        memory[root_table.value()..root_table.value() + A::PAGE_SIZE].fill(0);

        let mut s = Self {
            memory,
            frames_allocated: 1,
            root_table,
            phantom: std::marker::PhantomData,
        };

        for frame in 0..(size / A::PAGE_SIZE) {
            let phys = PhysicalAddress::new(frame * A::PAGE_SIZE);
            let virt = A::PHYSICAL_MEMORY_OFFSET + phys.value();
            s.map(virt, phys, PageFlags::new().with_writable(true));
        }

        let areas = vec![UsableMemoryRegion {
            base: PhysicalAddress::new(s.frames_allocated * A::PAGE_SIZE),
            size: s.memory.len() - s.frames_allocated * A::PAGE_SIZE,
        }];

        (s.memory, root_table, areas)
    }

    fn map(&mut self, virt: VirtualAddress, phys: PhysicalAddress, flags: PageFlags<A>) {
        let mut parent = self.root_table;
        for i in (1..A::PAGE_LEVELS).rev() {
            let index = A::index_at_level(virt, i);
            let start = parent.value() + index * A::PAGE_ENTRY_SIZE;

            let entry = unsafe {
                let ptr = self.memory[start..start + A::PAGE_ENTRY_SIZE].as_ptr() as *const usize;
                std::ptr::read(ptr)
            };
            let entry = unsafe { PageEntry::<A>::from_value(entry) };
            parent = match entry.address() {
                Ok(addr) => addr,
                Err(_) => {
                    let frame = self.allocate_frame();
                    let new_entry = PageEntry::<A>::new(frame, PageFlags::new_for_table());
                    let new_entry = new_entry.value();
                    unsafe {
                        let ptr = self.memory[start..start + A::PAGE_ENTRY_SIZE].as_mut_ptr()
                            as *mut usize;
                        std::ptr::write(ptr, new_entry);
                    }
                    frame
                }
            };
        }

        // Parent should now be the leaf table.
        let offset = A::index_at_level(virt, 0);
        let start = parent.value() + offset * A::PAGE_ENTRY_SIZE;
        let entry = unsafe { PageEntry::<A>::new(phys, flags).value() };
        unsafe {
            let ptr = self.memory[start..start + A::PAGE_ENTRY_SIZE].as_mut_ptr() as *mut usize;
            std::ptr::write(ptr, entry);
        }
    }

    fn allocate_frame(&mut self) -> PhysicalAddress {
        let frame = self.frames_allocated;
        self.frames_allocated += 1;
        let start = frame * A::PAGE_SIZE;
        if start + A::PAGE_SIZE > self.memory.len() {
            panic!("machine has insufficient memory to map the entire physical address space");
        }
        self.memory[start..start + A::PAGE_SIZE].fill(0);
        PhysicalAddress::new(start)
    }
}

#[cfg(test)]
mod test {
    mod x86_64 {
        use crate::{Architecture, EmulatedMachine, PhysicalAddress, UsableMemoryRegion, X8664};

        #[test]
        pub fn machine_initializes_physical_memory_table() {
            let (machine, _) = EmulatedMachine::<X8664>::new(
                64 * 1024 * 1024, // 64MiB
            );

            // Find the mapping for a random frame
            let frame = PhysicalAddress::new(7 * 1024 * 1024); // 7MiB
            let page = X8664::PHYSICAL_MEMORY_OFFSET + frame.value();

            let mut addr = machine.kernel_page_table_address;
            for level in (0..X8664::PAGE_LEVELS - 1).rev() {
                let offset = X8664::index_at_level(page, level);
                let start = addr.value() + offset * X8664::PAGE_ENTRY_SIZE;
                let entry = unsafe {
                    let ptr = machine.memory[start..start + X8664::PAGE_ENTRY_SIZE].as_ptr()
                        as *const usize;
                    std::ptr::read(ptr)
                };
                let entry = unsafe { crate::paging::PageEntry::<X8664>::from_value(entry) };
                addr = match entry.address() {
                    Ok(addr) => addr,
                    Err(_) => {
                        panic!(
                            "expected page table entry {:#o} to be present at {:#0x}",
                            offset, start
                        )
                    }
                };
            }

            assert_eq!(addr, frame);
        }

        #[test]
        pub fn building_machine_returns_usable_areas() {
            let (_, areas) = EmulatedMachine::<X8664>::new(
                64 * 1024 * 1024, // 64MiB
            );

            assert_eq!(
                vec![UsableMemoryRegion {
                    base: PhysicalAddress::new(0x22000),
                    size: 64 * 1024 * 1024 - 0x22000,
                }],
                areas
            );
        }
    }
}
