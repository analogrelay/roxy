use core::ptr;

use crate::{PhysicalAddress, VirtualAddress, paging::TableKind};

cfg_if::cfg_if! {
    if #[cfg(target_pointer_width = "64")] {
        mod x86_64;
        pub use x86_64::X8664;
    }
}

#[cfg(feature = "emulated")]
mod emulated;

#[cfg(feature = "emulated")]
pub use emulated::{Emulated, EmulatedMachine};

pub trait Architecture: Send + Sync {
    /// How far to right-shift an address to get the page number.
    ///
    /// This is the number of bits that are used to index WITHIN a page, and thus defines the size of a page.
    const PAGE_SHIFT: usize;

    /// How many bits are used to index a page table.
    const PAGE_ENTRY_SHIFT: usize;

    /// The number of page table levels in the architecture.
    const PAGE_LEVELS: usize;

    /// The width of an address in a page table entry.
    const ENTRY_ADDRESS_WIDTH: usize;

    /// How far to left-shift and address to get it into the correct position in a page table entry.
    ///
    /// Usually, this is the same as `PAGE_SHIFT`, which allows a page table entry to look similar to an address
    const ENTRY_ADDRESS_SHIFT: usize = Self::PAGE_SHIFT;

    /// The [`PageFlags`] flags values that represent the defaults for a page.
    const ENTRY_FLAG_DEFAULT_FOR_PAGE: usize;

    /// The [`PageFlags`] flags values that represent the defaults for a page table pointer.
    const ENTRY_FLAG_DEFAULT_FOR_TABLE: usize;

    /// The [`PageFlags`] flags value that indicates a page is present.
    const ENTRY_FLAG_PRESENT: usize;

    /// The [`PageFlags`] flags value that indicates a page is read-only.
    const ENTRY_FLAG_READONLY: usize;

    /// The [`PageFlags`] flags value that indicates a page is read/write.
    const ENTRY_FLAG_READWRITE: usize;

    /// The [`PageFlags`] flags value that indicates a page is not executable.
    const ENTRY_FLAG_NO_EXEC: usize;

    /// The [`PageFlags`] flags value that indicates a page is executable.
    const ENTRY_FLAG_EXEC: usize;

    /// The [`PageFlags`] flags value that indicates a page is not global.
    const ENTRY_FLAG_NO_GLOBAL: usize;

    /// The [`PageFlags`] flags value that indicates a page is global.
    const ENTRY_FLAG_GLOBAL: usize;

    /// The [`PageFlags`] flags value, used in a leaf page entry, that indicates a page is accessible in user mode.
    const ENTRY_FLAG_PAGE_USER: usize;

    /// The [`PageFlags`] flags value, used in a directory-level page entry, that indicates a page is accessible in user mode.
    const ENTRY_FLAG_TABLE_USER: usize = Self::ENTRY_FLAG_PAGE_USER;

    /// The virtual memory address at which all physical memory is mapped.
    ///
    /// Adding this offset to a physical address will yield a virtual address that can read/write this memory directly.
    ///
    /// NOTE: Using this may be unsafe if that physical memory is also mapped elsewhere.
    const PHYSICAL_MEMORY_OFFSET: VirtualAddress;

    /// The size of a page in bytes.
    const PAGE_SIZE: usize = 1 << Self::PAGE_SHIFT;

    /// A mask that can be used to get the offset within a page.
    const PAGE_OFFSET_MASK: usize = Self::PAGE_SIZE - 1;

    /// How many bits make up a virtual address
    const PAGE_ADDRESS_SHIFT: usize = Self::PAGE_LEVELS * Self::PAGE_ENTRY_SHIFT + Self::PAGE_SHIFT;

    /// The maximum virtual address that can be represented by the architecture.
    const PAGE_ADDRESS_SIZE: u64 = 1 << Self::PAGE_ADDRESS_SHIFT;

    /// A mask that can be used to get a virtual address from a single processor word.
    const PAGE_ADDRESS_MASK: usize = (Self::PAGE_ADDRESS_SIZE - (Self::PAGE_SIZE as u64)) as usize;

    /// The size of a page table entry in bytes.
    const PAGE_ENTRY_SIZE: usize = 1 << (Self::PAGE_SHIFT - Self::PAGE_ENTRY_SHIFT);

    /// The number of entries in a page table.
    const PAGE_ENTRIES: usize = 1 << Self::PAGE_ENTRY_SHIFT;

    /// A mask that can be used to get the index of a page table entry.
    const PAGE_ENTRY_MASK: usize = Self::PAGE_ENTRIES - 1;
    const PAGE_NEGATIVE_MASK: usize = !(Self::PAGE_ADDRESS_SIZE - 1) as usize;

    /// The size of physically-addressable memory in pages.
    const ENTRY_ADDRESS_SIZE: usize = 1 << Self::ENTRY_ADDRESS_WIDTH;

    /// Mask used to get the physical address from a page table entry.
    const ENTRY_ADDRESS_MASK: usize = Self::ENTRY_ADDRESS_SIZE - 1;

    /// Mask used to get the flags from a page table entry.
    const ENTRY_FLAGS_MASK: usize = !(Self::ENTRY_ADDRESS_MASK << Self::ENTRY_ADDRESS_SHIFT);

    const BIG_ENDIAN: bool;

    /// Returns a boolean indicating if a given address is valid.
    ///
    /// Many architectures have rules about what addresses are valid to use.
    /// For example, x86_64 requires that the upper 16 bits of the address
    /// are either all 0 or all 1. This function returns true if the address
    /// is valid and false otherwise.
    ///
    /// Note: A valid address may still refer to an unmapped page!
    fn is_valid(&self, address: VirtualAddress) -> bool;

    /// Validates the flags for a page table entry.
    ///
    /// Returns `true` if the flags are valid, `false` otherwise.
    fn validate_flags(flags: usize) -> bool;

    /// Gets the address of the current root page table of the specified kind.
    ///
    /// Not all architectures allow a separate user and kernel page table, so the [`table_kind`] may be ignored.
    fn page_table_address(&self, table_kind: TableKind) -> PhysicalAddress;

    /// Invalidates the translation lookaside buffer for the given address.
    fn invalidate_one(&self, addr: VirtualAddress);

    /// Clears the entire translation lookaside buffer.
    fn invalidate_all(&self, table_kind: TableKind);

    /// Extracts the base address of the provided address at the given page table level.
    fn base_at_level(addr: VirtualAddress, level: usize) -> VirtualAddress {
        assert!(level < Self::PAGE_LEVELS);
        let mask = (Self::PAGE_ENTRIES - 1) << Self::PAGE_ENTRY_SHIFT * level + Self::PAGE_SHIFT;
        VirtualAddress::new(addr.value() & mask)
    }

    /// Extracts the index covering this address at the given page table level.
    fn index_at_level(addr: VirtualAddress, level: usize) -> usize {
        let val = addr.value() >> Self::PAGE_ENTRY_SHIFT * level + Self::PAGE_SHIFT;
        val & Self::PAGE_ENTRY_MASK
    }

    /// Gets the number of bytes spanned by a single page table entry at the given level.
    fn span_at_level(level: usize) -> usize {
        Self::PAGE_SIZE << (Self::PAGE_ENTRY_SHIFT * level)
    }

    /// Sets the address of the current root page table of the specified kind.
    ///
    /// Not all architectures allow a separate user and kernel page table, so the [`table_kind`] may be ignored.
    unsafe fn set_page_table_address(&self, table_kind: TableKind, address: PhysicalAddress);

    /// Converts a physical address to a virtual address.
    fn virtual_address_for(&self, address: PhysicalAddress) -> VirtualAddress {
        match Self::PHYSICAL_MEMORY_OFFSET
            .value()
            .checked_add(address.value())
        {
            Some(value) => VirtualAddress::new(value),
            None => panic!("overflow when converting physical address to virtual address"),
        }
    }

    /// Reads a value to the memory at the given address.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the address is valid and contains a valid value.
    unsafe fn read<T>(&self, address: VirtualAddress) -> T {
        unsafe {
            // SAFETY: The caller must ensure that the address is valid.
            ptr::read(address.as_mut_ptr() as *mut T)
        }
    }

    /// Writes a value to the memory at the given address.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the address is valid and does not represent an existing value.
    unsafe fn write<T>(&self, address: VirtualAddress, value: T) {
        unsafe {
            // SAFETY: The caller must ensure that the address is valid.
            ptr::write(address.as_mut_ptr() as *mut T, value);
        }
    }

    /// Writes `count` bytes of `value` to the memory starting at `address`.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the address is valid and does not represent an existing value.
    unsafe fn write_bytes(&self, address: VirtualAddress, value: u8, count: usize) {
        unsafe {
            // SAFETY: The caller must ensure that the address is valid.
            ptr::write_bytes(address.as_mut_ptr() as *mut u8, value, count);
        }
    }
}
