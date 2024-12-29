//! Contains frame allocators used by the kernel.
//!
//! There are two frame allocators in this module:
//! * [`BumpFrameAllocator`] is a frame allocator that simply allocates the next free frame.
//! It is incapable of freeing frames.
//! It's used for the very first few allocations during boot, which are never freed.
mod bump;

use core::ops::Range;

pub use bump::BumpFrameAllocator;

use crate::{Error, PhysicalAddress};

/// A structure representing the number of frames available and the number of frames in use.
pub struct FrameUsage {
    pub total: usize,
    pub used: usize,
}

impl FrameUsage {
    /// Returns the number of frames available.
    pub fn available(&self) -> usize {
        self.total - self.used
    }
}

pub trait FrameAllocator {
    /// Allocates `count` frames of contiguous physical memory.
    unsafe fn allocate_frame(&mut self, count: usize) -> Result<Range<PhysicalAddress>, Error>;

    /// Allocates a single frame of physical memory.
    unsafe fn allocate_one(&mut self) -> Result<PhysicalAddress, Error> {
        unsafe { self.allocate_frame(1).map(|r| r.start) }
    }

    /// Frees `count` frames of contiguous physical memory starting at `address`.
    ///
    /// # Safety
    ///
    /// Attempting to free an area of memory that is currently in use will result in undefined
    /// behavior.
    ///
    /// # Panics
    ///
    /// This function may panic if:
    /// * The allocator is incapable of freeing frames.
    /// * The `address` is not a valid frame address.
    /// * The indicated region was not allocated by this allocator.
    unsafe fn free(&mut self, address: PhysicalAddress, count: usize) -> Result<(), Error>;

    /// Retrieves the total number of frames available and the number of frames in use.
    unsafe fn usage(&self) -> FrameUsage;
}
