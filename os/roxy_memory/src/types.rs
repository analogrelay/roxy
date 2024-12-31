use core::{
    fmt::Debug,
    ops::{Add, AddAssign, Rem, Sub, SubAssign},
};

/// Indicates a "zone" of memory, which can be either [`MemoryZone::Kernel`], to represent
/// kernel memory, or [`MemoryZone::User`], to represent user memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryZone {
    Kernel,
    User,
}

/// Represents a region of usable memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryRegion {
    pub base: PhysicalAddress,
    pub size: usize,
}

impl MemoryRegion {
    #[inline(always)]
    pub fn end(&self) -> PhysicalAddress {
        self.base + self.size
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysicalAddress(usize);

impl PhysicalAddress {
    #[inline(always)]
    pub const fn new(addr: usize) -> Self {
        Self(addr)
    }

    #[inline(always)]
    pub const fn value(&self) -> usize {
        self.0
    }

    #[inline(always)]
    pub const fn align_up(&self, align: usize) -> Self {
        Self(self.0.next_multiple_of(align))
    }
}

impl Rem<usize> for PhysicalAddress {
    type Output = usize;

    #[inline(always)]
    fn rem(self, rhs: usize) -> Self::Output {
        self.0 % rhs
    }
}

impl Add<usize> for PhysicalAddress {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl AddAssign<usize> for PhysicalAddress {
    fn add_assign(&mut self, rhs: usize) {
        self.0 += rhs;
    }
}

impl Sub<usize> for PhysicalAddress {
    type Output = Self;

    #[inline(always)]
    fn sub(self, rhs: usize) -> Self::Output {
        Self(self.0 - rhs)
    }
}

impl Sub<PhysicalAddress> for PhysicalAddress {
    type Output = usize;

    #[inline(always)]
    fn sub(self, rhs: PhysicalAddress) -> Self::Output {
        self.0 - rhs.0
    }
}

impl SubAssign<usize> for PhysicalAddress {
    fn sub_assign(&mut self, rhs: usize) {
        self.0 -= rhs;
    }
}

impl Debug for PhysicalAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("[{:#x} phys]", self.0))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtualAddress(usize);

impl VirtualAddress {
    #[inline(always)]
    pub const fn new(addr: usize) -> Self {
        Self(addr)
    }

    #[inline(always)]
    pub const fn value(&self) -> usize {
        self.0
    }

    pub const fn zone(&self) -> MemoryZone {
        // If we treat the address as signed, then negative values indicate kernel memory.
        if (self.0 as isize) < 0 {
            MemoryZone::Kernel
        } else {
            MemoryZone::User
        }
    }

    pub fn as_mut_ptr<T>(&self) -> *mut T {
        self.0 as *mut T
    }

    #[inline(always)]
    pub const fn align_up(&self, align: usize) -> Self {
        Self(self.0.next_multiple_of(align))
    }
}

impl<T> From<*const T> for VirtualAddress {
    fn from(ptr: *const T) -> Self {
        Self(ptr as usize)
    }
}

impl<T> From<*mut T> for VirtualAddress {
    fn from(ptr: *mut T) -> Self {
        Self(ptr as usize)
    }
}

impl Rem<usize> for VirtualAddress {
    type Output = usize;

    #[inline(always)]
    fn rem(self, rhs: usize) -> Self::Output {
        self.0 % rhs
    }
}

impl Add<usize> for VirtualAddress {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl Sub<usize> for VirtualAddress {
    type Output = Self;

    #[inline(always)]
    fn sub(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl Debug for VirtualAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("[{:#x} virt]", self.0))
    }
}
