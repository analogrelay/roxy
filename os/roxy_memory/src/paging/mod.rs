mod entry;
mod flags;
mod mapper;
mod table;

pub use entry::{PageEntry, PageEntryAddress};
pub use flags::PageFlags;
pub use mapper::PageMapper;
pub use table::PageTable;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableKind {
    Kernel,
    User,
}
