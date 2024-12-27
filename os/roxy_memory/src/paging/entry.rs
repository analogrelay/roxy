use core::{fmt::Debug, marker::PhantomData};

use crate::{Architecture, PhysicalAddress, paging::PageFlags};

pub enum PageEntryAddress {
    /// The entry is present, and the address is the physical address of the page.
    Present(PhysicalAddress),

    /// The entry is not present. The value is the OS-specific value stored as the address.
    NotPresent(usize),
}

pub struct PageEntry<A> {
    value: usize,
    phantom: PhantomData<A>,
}

impl<A: Architecture> Clone for PageEntry<A> {
    fn clone(&self) -> Self {
        Self {
            value: self.value,
            phantom: PhantomData,
        }
    }
}

impl<A: Architecture> Copy for PageEntry<A> {}

impl<A: Architecture> PartialEq for PageEntry<A> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<A: Architecture> Eq for PageEntry<A> {}

impl<A: Architecture> Debug for PageEntry<A> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PageEntry")
            .field("address", &self.address())
            .field("flags", &self.flags())
            .field("value", &self.value)
            .finish()
    }
}

impl<A: Architecture> PageEntry<A> {
    /// Creates a new page entry from an address and flags.
    ///
    /// # Safety
    ///
    /// The flags must be valid for the architecture.
    #[inline(always)]
    pub fn new(address: PhysicalAddress, flags: PageFlags<A>) -> Self {
        let flags = flags.value();
        let address = address.value();
        let value = (((address >> A::PAGE_SHIFT) & A::ENTRY_ADDRESS_MASK)
            << A::ENTRY_ADDRESS_SHIFT)
            | (flags & A::ENTRY_FLAGS_MASK);
        unsafe {
            // SAFETY: We constructed the value, so it should be valid.
            Self::from_value(value)
        }
    }

    /// Creates a new page entry from a raw value.
    ///
    /// # Safety
    ///
    /// The value must be a valid page entry value.
    #[inline(always)]
    pub unsafe fn from_value(value: usize) -> PageEntry<A> {
        Self {
            value,
            phantom: PhantomData,
        }
    }

    /// Gets the raw value of the page entry.
    #[inline(always)]
    pub fn value(&self) -> usize {
        self.value
    }

    /// Gets the address stored in the page entry.
    ///
    /// Returns `Ok(addr)` if the entry is present, with `addr` being the physical address pointed to by the entry.
    /// Returns `Err(value)` if the entry is not present, with `value` being the OS-specific value stored as the address.
    #[inline(always)]
    pub fn address(&self) -> Result<PhysicalAddress, usize> {
        let addr = PhysicalAddress::new(
            ((self.value >> A::ENTRY_ADDRESS_SHIFT) & A::ENTRY_ADDRESS_MASK) << A::PAGE_SHIFT,
        );

        if self.present() {
            Ok(addr)
        } else {
            Err(self.value)
        }
    }

    /// Gets the flags of the page entry.
    #[inline(always)]
    pub fn flags(&self) -> PageFlags<A> {
        unsafe {
            // SAFETY: The flags should have been set using PageFlags, so they should be valid PageFlags.
            PageFlags::from_flags(self.value & A::ENTRY_FLAGS_MASK)
        }
    }

    /// Sets the flags of the page entry.
    #[inline(always)]
    pub fn set_flags(&mut self, flags: PageFlags<A>) {
        self.value = (self.value & !A::ENTRY_FLAGS_MASK) | flags.value();
    }

    /// Shorthand for checking if the present flag is set.
    #[inline(always)]
    pub fn present(&self) -> bool {
        self.value & A::ENTRY_FLAG_PRESENT != 0
    }
}
