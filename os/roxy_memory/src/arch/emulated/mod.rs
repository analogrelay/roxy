use std::{
    ops::{Deref, DerefMut},
    sync::RwLock,
};

pub use machine::EmulatedMachine;

use crate::{Error, PhysicalAddress, VirtualAddress, arch::Architecture};

mod machine;

/// Represents an emulated x86-64 architecture used for testing.
pub struct Emulated<A> {
    arch: A,
    machine: RwLock<EmulatedMachine<A>>,
}

impl<A: Architecture> Emulated<A> {
    pub const ROOT_PAGE_TABLE_ADDRESS: PhysicalAddress = PhysicalAddress::new(0x2000);

    pub fn new(arch: A, machine: EmulatedMachine<A>) -> Self {
        let this = Self {
            arch,
            machine: RwLock::new(machine),
        };

        this
    }

    pub fn machine(&self) -> impl Deref<Target = EmulatedMachine<A>> {
        self.machine.read().unwrap()
    }

    pub fn machine_mut(&mut self) -> impl DerefMut<Target = EmulatedMachine<A>> {
        self.machine.write().unwrap()
    }

    /// Clears the poison flag on the machine lock.
    ///
    /// Used in tests to reset the poison state when a panic occurs.
    pub fn clear_poison(&self) {
        self.machine.clear_poison();
    }

    pub unsafe fn try_read<T>(&self, address: VirtualAddress) -> Result<T, Error> {
        // Need write access when reading in order to update the TLB
        self.machine.write()?.read(address)
    }

    pub unsafe fn try_write<T>(&self, address: VirtualAddress, value: T) -> Result<(), Error> {
        self.machine.write()?.write(address, value)
    }
}

impl<A: Architecture> Architecture for Emulated<A> {
    const PAGE_SHIFT: usize = A::PAGE_SHIFT;
    const PAGE_ENTRY_SHIFT: usize = A::PAGE_ENTRY_SHIFT;
    const PAGE_LEVELS: usize = A::PAGE_LEVELS;
    const ENTRY_ADDRESS_WIDTH: usize = A::ENTRY_ADDRESS_WIDTH;
    const PHYSICAL_MEMORY_OFFSET: VirtualAddress = A::PHYSICAL_MEMORY_OFFSET;
    const ENTRY_FLAG_DEFAULT_FOR_PAGE: usize = A::ENTRY_FLAG_DEFAULT_FOR_PAGE;
    const ENTRY_FLAG_DEFAULT_FOR_TABLE: usize = A::ENTRY_FLAG_DEFAULT_FOR_TABLE;
    const ENTRY_FLAG_PRESENT: usize = A::ENTRY_FLAG_PRESENT;
    const ENTRY_FLAG_READONLY: usize = A::ENTRY_FLAG_READONLY;
    const ENTRY_FLAG_READWRITE: usize = A::ENTRY_FLAG_READWRITE;
    const ENTRY_FLAG_NO_EXEC: usize = A::ENTRY_FLAG_NO_EXEC;
    const ENTRY_FLAG_EXEC: usize = A::ENTRY_FLAG_EXEC;
    const ENTRY_FLAG_NO_GLOBAL: usize = A::ENTRY_FLAG_NO_GLOBAL;
    const ENTRY_FLAG_GLOBAL: usize = A::ENTRY_FLAG_GLOBAL;
    const ENTRY_FLAG_PAGE_USER: usize = A::ENTRY_FLAG_PAGE_USER;
    const BIG_ENDIAN: bool = A::BIG_ENDIAN;

    fn is_valid(&self, address: VirtualAddress) -> bool {
        self.arch.is_valid(address)
    }

    fn validate_flags(flags: usize) -> bool {
        A::validate_flags(flags)
    }

    unsafe fn write_bytes(&self, address: crate::VirtualAddress, value: u8, count: usize) {
        self.machine
            .write()
            .unwrap()
            .write_bytes(address, value, count)
            .unwrap();
    }

    fn page_table_address(&self, table_kind: crate::paging::TableKind) -> PhysicalAddress {
        self.machine.read().unwrap().page_table_address(table_kind)
    }

    unsafe fn set_page_table_address(
        &self,
        table_kind: crate::paging::TableKind,
        address: PhysicalAddress,
    ) {
        self.machine
            .write()
            .unwrap()
            .set_page_table_address(table_kind, address)
    }

    fn invalidate_one(&self, addr: VirtualAddress) {
        self.machine.write().unwrap().invalidate_one(addr)
    }

    fn invalidate_all(&self, table_kind: crate::paging::TableKind) {
        self.machine.write().unwrap().invalidate_all(table_kind);
    }

    unsafe fn read<T>(&self, address: VirtualAddress) -> T {
        // Need write access when reading in order to update the TLB
        unsafe { self.try_read(address).unwrap() }
    }

    unsafe fn write<T>(&self, address: VirtualAddress, value: T) {
        unsafe { self.try_write(address, value).unwrap() };
    }
}
