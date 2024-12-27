use core::marker::PhantomData;

use crate::Architecture;

pub struct PageFlags<A> {
    value: usize,
    phantom: PhantomData<A>,
}

impl<A: Architecture> Clone for PageFlags<A> {
    fn clone(&self) -> Self {
        unsafe { Self::from_flags(self.value) }
    }
}

impl<A: Architecture> Copy for PageFlags<A> {}

impl<A: Architecture> PartialEq for PageFlags<A> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<A: Architecture> Eq for PageFlags<A> {}

impl<A: Architecture> PageFlags<A> {
    /// Creates new page flags matching the architecture-defined default
    /// for read-only, no-execute, kernel pages.
    #[inline(always)]
    pub fn new() -> Self {
        unsafe {
            // SAFETY: The default flags are architecture-defined, so they should be safe to use.
            Self::from_flags(
                A::ENTRY_FLAG_DEFAULT_FOR_PAGE
                    | A::ENTRY_FLAG_READONLY
                    | A::ENTRY_FLAG_NO_EXEC
                    | A::ENTRY_FLAG_NO_GLOBAL,
            )
        }
    }

    /// Creates new page flags matching the architecture-defined default
    /// for further page tables.
    #[inline(always)]
    pub fn new_for_table() -> Self {
        unsafe {
            // SAFETY: The default flags are architecture-defined, so they should be safe to use.
            Self::from_flags(
                A::ENTRY_FLAG_DEFAULT_FOR_TABLE | A::ENTRY_FLAG_NO_EXEC | A::ENTRY_FLAG_NO_GLOBAL,
            )
        }
    }

    /// Creates new page flags from a raw value.
    ///
    /// # Safety
    ///
    /// The value must be a valid set of flags for the architecture.
    #[inline(always)]
    pub unsafe fn from_flags(value: usize) -> Self {
        Self {
            value,
            phantom: PhantomData,
        }
    }

    #[inline(always)]
    pub fn value(&self) -> usize {
        self.value
    }

    #[inline(always)]
    pub fn with_flag(mut self, flag: usize, value: bool) -> Self {
        if value {
            self.value |= flag;
        } else {
            self.value &= !flag;
        }
        self
    }

    #[inline(always)]
    pub fn flag(&self, flag: usize) -> bool {
        self.value & flag != 0
    }

    #[inline(always)]
    pub fn present(&self) -> bool {
        self.flag(A::ENTRY_FLAG_PRESENT)
    }

    #[inline(always)]
    #[must_use]
    pub fn with_user(self, value: bool) -> Self {
        self.with_flag(A::ENTRY_FLAG_PAGE_USER, value)
    }

    #[inline(always)]
    pub fn user(&self) -> bool {
        self.flag(A::ENTRY_FLAG_PAGE_USER)
    }

    #[inline(always)]
    #[must_use]
    pub fn with_writable(self, value: bool) -> Self {
        // Architectures may have different flags for read-only and read-write.
        // So clear both flags and set the correct one.
        // If an architecture doesn't support one of these flags,
        // it will be '0' and ORing it or setting will have no effect.
        if value {
            self.with_flag(A::ENTRY_FLAG_READONLY | A::ENTRY_FLAG_READWRITE, false)
                .with_flag(A::ENTRY_FLAG_READWRITE, true)
        } else {
            self.with_flag(A::ENTRY_FLAG_READONLY | A::ENTRY_FLAG_READWRITE, false)
                .with_flag(A::ENTRY_FLAG_READONLY, true)
        }
    }

    #[inline(always)]
    pub fn writable(&self) -> bool {
        // Check both flags, as an architecture may use both.
        // If a flag isn't used, it will be '0' and ORing it will have no effect.
        // We compare to the value that indicates read-write, so that on architectures
        // that only support a read-only flag, we'll only return true if that flag is clear.
        self.value & (A::ENTRY_FLAG_READWRITE | A::ENTRY_FLAG_READONLY) == A::ENTRY_FLAG_READWRITE
    }

    #[inline(always)]
    #[must_use]
    pub fn with_executable(self, value: bool) -> Self {
        // We have to support architectures that use a no-execute flag,
        // as well as those that use an execute flag.
        self.with_flag(A::ENTRY_FLAG_NO_EXEC, !value)
            .with_flag(A::ENTRY_FLAG_EXEC, value)
    }

    #[inline(always)]
    pub fn executable(&self) -> bool {
        // Check both flags, as an architecture may use both.
        // If a flag isn't used, it will be '0' and ORing it will have no effect.
        // We compare to the value that indicates execute, so that on architectures
        // that only support a no-execute flag, we'll only return true if that flag is clear.
        self.value & (A::ENTRY_FLAG_EXEC | A::ENTRY_FLAG_NO_EXEC) == A::ENTRY_FLAG_EXEC
    }

    #[inline(always)]
    #[must_use]
    pub fn with_global(self, value: bool) -> Self {
        // We have to support architectures that use a no-global flag,
        // as well as those that use a global flag.
        self.with_flag(A::ENTRY_FLAG_NO_GLOBAL, !value)
            .with_flag(A::ENTRY_FLAG_GLOBAL, value)
    }

    #[inline(always)]
    pub fn global(&self) -> bool {
        // Check both flags, as an architecture may use both.
        // If a flag isn't used, it will be '0' and ORing it will have no effect.
        // We compare to the value that indicates global, so that on architectures
        // that only support a no-global flag, we'll only return true if that flag is clear.
        self.value & (A::ENTRY_FLAG_GLOBAL | A::ENTRY_FLAG_NO_GLOBAL) == A::ENTRY_FLAG_GLOBAL
    }
}

impl<A: Architecture> core::fmt::Debug for PageFlags<A> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PageFlags")
            .field("present", &self.present())
            .field("writable", &self.writable())
            .field("executable", &self.executable())
            .field("global", &self.global())
            .field("user", &self.user())
            .field("bits", &format_args!("{:#0x}", self.value))
            .finish()
    }
}

#[cfg(test)]
mod test {
    use crate::{X8664, paging::PageFlags};

    #[test]
    pub fn x86_64() {
        assert_eq!(
            // eXecute Disable (XD), and Present (P) bits
            unsafe { PageFlags::<X8664>::from_flags(0x8000_0000_0000_0001) },
            PageFlags::<X8664>::new()
        );
        assert_eq!(
            // XD, P, and Read/Write (RW) bits
            unsafe { PageFlags::<X8664>::from_flags(0x8000_0000_0000_0003) },
            PageFlags::<X8664>::new_for_table()
        );
        assert_eq!(
            // Just the P bit (XD is off)
            unsafe { PageFlags::<X8664>::from_flags(0x0000_0000_0000_0001) },
            PageFlags::<X8664>::new().with_executable(true)
        );
        assert_eq!(
            // P, and RW bits
            unsafe { PageFlags::<X8664>::from_flags(0x8000_0000_0000_0003) },
            PageFlags::<X8664>::new().with_writable(true)
        );
        assert_eq!(
            // User-accessible (U), and P bits
            unsafe { PageFlags::<X8664>::from_flags(0x8000_0000_0000_0005) },
            PageFlags::<X8664>::new().with_user(true)
        );
        assert_eq!(
            // Global (G), and P bits
            unsafe { PageFlags::<X8664>::from_flags(0x8000_0000_0000_0101) },
            PageFlags::<X8664>::new().with_global(true)
        );
    }
}
