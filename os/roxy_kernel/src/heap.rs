use linked_list_allocator::LockedHeap;

#[cfg_attr(not(test), global_allocator)]
pub static ALLOCATOR: LockedHeap = LockedHeap::empty();
