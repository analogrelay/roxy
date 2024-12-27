use thiserror::Error;
use x86_64::{
    structures::paging::{mapper::MapToError, PageSize},
    PhysAddr,
};

#[derive(Error, Debug)]
pub enum Error {
    #[error("unable to find a free physical frame to allocate")]
    OutOfPhysicalMemory,
    #[error("the parent entry is a huge page")]
    ParentEntryHugePage,
    #[error("the page is already mapped to {frame_start:#08X}")]
    PageAlreadyMapped { frame_start: PhysAddr, size: u64 },
}

pub type Result<T> = ::core::result::Result<T, Error>;

impl<S: PageSize> From<MapToError<S>> for Error {
    fn from(error: MapToError<S>) -> Self {
        match error {
            MapToError::FrameAllocationFailed => Error::OutOfPhysicalMemory,
            MapToError::ParentEntryHugePage => Error::ParentEntryHugePage,
            MapToError::PageAlreadyMapped(frame) => Error::PageAlreadyMapped {
                frame_start: frame.start_address(),
                size: S::SIZE,
            },
        }
    }
}
