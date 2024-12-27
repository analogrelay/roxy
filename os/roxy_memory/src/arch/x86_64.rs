use core::arch::asm;

use crate::{PhysicalAddress, VirtualAddress, arch::Architecture, paging::TableKind};

#[inline(always)]
pub fn is_canonical(address: usize) -> bool {
    let sign_and_extension = address & 0xFFFF_8000_0000_0000;

    // Either the sign bit is clear and the upper bits are clear
    // Or the sign bit is set and the upper bits are set

    // TODO: We don't handle 5-level paging here
    return sign_and_extension == 0 || sign_and_extension == 0xFFFF_8000_0000_0000;
}

pub struct X8664;

impl Architecture for X8664 {
    const PAGE_SHIFT: usize = 12;
    const PAGE_ENTRY_SHIFT: usize = 9;
    const PAGE_LEVELS: usize = 4; // TODO: Support 5-level paging?

    const ENTRY_ADDRESS_WIDTH: usize = 40;

    // The entire upper half of the address space contains the physical memory mapping.
    const PHYSICAL_MEMORY_OFFSET: VirtualAddress =
        VirtualAddress::new(Self::PAGE_NEGATIVE_MASK + (Self::PAGE_ADDRESS_SIZE >> 1) as usize);

    const ENTRY_FLAG_DEFAULT_FOR_PAGE: usize = Self::ENTRY_FLAG_PRESENT;
    const ENTRY_FLAG_DEFAULT_FOR_TABLE: usize =
        Self::ENTRY_FLAG_PRESENT | Self::ENTRY_FLAG_READWRITE;
    const ENTRY_FLAG_PRESENT: usize = 1 << 0;
    const ENTRY_FLAG_READONLY: usize = 0;
    const ENTRY_FLAG_READWRITE: usize = 1 << 1;
    const ENTRY_FLAG_PAGE_USER: usize = 1 << 2;
    const ENTRY_FLAG_GLOBAL: usize = 1 << 8;
    const ENTRY_FLAG_NO_GLOBAL: usize = 0;
    const ENTRY_FLAG_EXEC: usize = 0;
    const ENTRY_FLAG_NO_EXEC: usize = 1 << 63;

    const BIG_ENDIAN: bool = false;

    fn is_valid(&self, address: VirtualAddress) -> bool {
        is_canonical(address.value())
    }

    #[inline(always)]
    fn page_table_address(
        &self,
        // x86_64 has only one page table kind
        _table_kind: crate::paging::TableKind,
    ) -> crate::PhysicalAddress {
        unsafe {
            // SAFETY: We're assigning to address within this block.
            let address: usize;
            asm!("mov {0}, cr3", out(reg) address);
            PhysicalAddress::new(address)
        }
    }

    #[inline(always)]
    unsafe fn set_page_table_address(
        &self,
        // x86_64 has only one page table
        _table_kind: crate::paging::TableKind,
        address: PhysicalAddress,
    ) {
        unsafe {
            // SAFETY: The caller is asserting that this is a valid physical address.
            asm!("mov cr3, {0}", in(reg) address.value());
        }
    }

    fn invalidate_one(&self, addr: VirtualAddress) {
        // SAFETY: Invalidating the TLB cache is "safe" from a memory safety perspective.
        unsafe { asm!("invlpg [{0}]", in(reg) addr.value()) };
    }

    fn invalidate_all(&self, table_kind: TableKind) {
        // Resetting the page table address will invalidate the TLB
        unsafe { self.set_page_table_address(table_kind, self.page_table_address(table_kind)) };
    }
}

#[cfg(test)]
mod test {
    use crate::{
        VirtualAddress,
        arch::{Architecture, X8664},
    };

    #[test]
    pub fn validate_constants() {
        assert_eq!(X8664::PAGE_SIZE, 4096);
        assert_eq!(X8664::PAGE_OFFSET_MASK, 0xFFF);
        assert_eq!(X8664::PAGE_ADDRESS_SHIFT, 48);
        assert_eq!(X8664::PAGE_ADDRESS_SIZE, 0x0001_0000_0000_0000);
        assert_eq!(X8664::PAGE_ADDRESS_MASK, 0x0000_FFFF_FFFF_F000);
        assert_eq!(X8664::PAGE_ENTRY_SIZE, 8);
        assert_eq!(X8664::PAGE_ENTRIES, 512);
        assert_eq!(X8664::PAGE_ENTRY_MASK, 0x1FF);
        assert_eq!(X8664::PAGE_NEGATIVE_MASK, 0xFFFF_0000_0000_0000);
        assert_eq!(X8664::ENTRY_ADDRESS_SIZE, 0x0000_0100_0000_0000);
        assert_eq!(X8664::ENTRY_ADDRESS_MASK, 0x0000_00FF_FFFF_FFFF);
        assert_eq!(X8664::ENTRY_FLAGS_MASK, 0xFFF0_0000_0000_0FFF);

        const TEST_ADDR: VirtualAddress = VirtualAddress::new(0o123_456_701_234_5670);
        assert_eq!(
            VirtualAddress::new(0o000_000_000_234_0000),
            X8664::base_at_level(TEST_ADDR, 0),
        );
        assert_eq!(0o234, X8664::index_at_level(TEST_ADDR, 0),);
        assert_eq!(0o1_0000, X8664::span_at_level(0),);
        assert_eq!(
            VirtualAddress::new(0o000_000_701_000_0000),
            X8664::base_at_level(TEST_ADDR, 1),
        );
        assert_eq!(0o1_000_0000, X8664::span_at_level(1),);
        assert_eq!(0o701, X8664::index_at_level(TEST_ADDR, 1),);
        assert_eq!(
            VirtualAddress::new(0o000_456_000_000_0000),
            X8664::base_at_level(TEST_ADDR, 2),
        );
        assert_eq!(0o1_000_000_0000, X8664::span_at_level(2),);
        assert_eq!(0o456, X8664::index_at_level(TEST_ADDR, 2),);
        assert_eq!(
            VirtualAddress::new(0o123_000_000_000_0000),
            X8664::base_at_level(TEST_ADDR, 3),
        );
        assert_eq!(0o1_000_000_000_0000, X8664::span_at_level(3),);
        assert_eq!(0o123, X8664::index_at_level(TEST_ADDR, 3),);

        assert_eq!(
            X8664::PHYSICAL_MEMORY_OFFSET,
            VirtualAddress::new(0xFFFF_8000_0000_0000)
        );
    }

    #[test]
    pub fn is_valid() {
        fn valid(address: usize) -> bool {
            X8664.is_valid(VirtualAddress::new(address))
        }
        fn invalid(address: usize) -> bool {
            !X8664.is_valid(VirtualAddress::new(address))
        }

        assert!(valid(0x0000_0000_0000_0000));
        assert!(valid(0x0000_1234_5678_9abc));
        assert!(valid(0xFFFF_8000_0000_0000));
        assert!(valid(0xFFFF_9123_4567_89ab));

        assert!(invalid(0x0000_9123_4567_89ab));
        assert!(invalid(0xFFFF_1234_5678_9abc));
    }
}
