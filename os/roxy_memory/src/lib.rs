#![no_std]

#[cfg(any(test, feature = "std"))]
extern crate std;

extern crate alloc;

mod allocator;
mod arch;
pub mod paging;
mod types;

pub use allocator::*;
pub use arch::*;
use thiserror::Error;
pub use types::*;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("attempted to access an invalid page table entry: {0}")]
    PageTableIndexOutOfRange(usize),
    #[error("page table is a leaf table and contains no sub-tables")]
    PageTableIsLeaf,
    #[error("attempted to take a poisoned lock")]
    LockPoisoned,
    #[error(
        "attempted to map a region of virtual addresses to a region of physical addresses with a different size"
    )]
    RegionSizesNotEqual,
    #[error("the address {0:#0x} is not page-aligned")]
    NotPageAligned(usize),
    #[error("out of physical memory")]
    OutOfPhysicalMemory,
    #[error("page is read-only, but write access was attempted")]
    PageIsReadOnly,
    #[error("page is not mapped")]
    PageNotMapped,
    #[error("physical address is outside of the usable memory region")]
    PhysicalAddressOutOfRange,
    #[error("page flags are invalid: {0:#0x}")]
    InvalidPageFlags(usize),
    #[error("bump allocator cannot free memory")]
    BumpAllocatorCannotFree,
}

#[cfg(feature = "std")]
impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        Error::LockPoisoned
    }
}
