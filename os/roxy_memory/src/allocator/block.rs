use core::{fmt::Debug, mem::MaybeUninit, ptr::NonNull};

use crate::PhysicalAddress;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct MemoryBlockFlags: u64 {
        // A test flag for use in unit tests.
        // TODO: Remove this when we have real flags we can set in tests.
        const TEST = 1 << 0;
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct MemoryBlock {
    pub base: PhysicalAddress,
    pub size: usize,
    pub flags: MemoryBlockFlags,
}

impl MemoryBlock {
    #[inline(always)]
    pub fn end(&self) -> PhysicalAddress {
        self.base + self.size
    }
}

impl Debug for MemoryBlock {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MemoryBlock")
            .field("base", &self.base)
            .field("end", &self.end())
            .field("size", &self.size)
            .field("flags", &self.flags)
            .finish()
    }
}

/// Represents an ordered list of non-overlapping memory blocks.
struct MemoryBlockList {
    pub ptr: *mut MemoryBlock,
    pub len: usize,
    pub cap: usize,
}

impl MemoryBlockList {
    /// Initializes a new `MemoryBlockList` from an array of `MaybeUninit<MemoryBlock>`.
    ///
    /// # Safety
    ///
    /// This is unsafe because the caller must ensure the array is only usable by this list.
    pub unsafe fn from_array<const SIZE: usize>(
        array: *mut [MaybeUninit<MemoryBlock>; SIZE],
    ) -> Self {
        let ptr = array as *mut MemoryBlock;
        Self {
            ptr,
            len: 0,
            cap: SIZE,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn capacity(&self) -> usize {
        self.cap
    }

    pub fn get(&self, index: usize) -> Option<&MemoryBlock> {
        if index >= self.len {
            return None;
        }
        unsafe { self.ptr.add(index).as_ref() }
    }

    #[cfg(any(test, feature = "std"))]
    pub fn into_vec(self) -> std::vec::Vec<MemoryBlock> {
        let mut vec = std::vec::Vec::with_capacity(self.len);
        for i in 0..self.len {
            unsafe {
                vec.push(self.ptr.add(i).read());
            }
        }
        vec
    }

    /// Try to insert a new block into the list.
    ///
    /// Blocks will be split and merged as necessary to ensure the list remains sorted and non-overlapping.
    /// If inserting this block would cause the list to exceed its capacity, an error is returned reporting the total amount of space needed.
    #[tracing::instrument(level = "debug", skip(self))]
    pub fn try_insert(
        &mut self,
        new_block: MemoryBlock,
        mut capacity_verified: bool,
    ) -> Result<(), usize> {
        // Special case for an empty list
        if self.len == 0 {
            tracing::debug!("empty list, inserting block");
            // There's no need to check capacity or split blocks, just write the block to the first position.
            unsafe { self.ptr.write(new_block) };
            self.len += 1;
            return Ok(());
        }

        if self.cap >= self.len * 2 + 1 {
            // We have enough capacity to split every single block, so we know we have capacity to insert this block.
            capacity_verified = true;
        }

        let mut new_region_count = 0;

        let mut base = new_block.base;
        let mut merge_start = None;
        let mut merge_end = None;

        let mut idx = 0;
        while idx < self.len {
            let current = unsafe { self.ptr.add(idx).read() };
            let _span =
                tracing::debug_span!("consider_block", index = idx, current = ?current).entered();
            if current.base >= new_block.end() {
                // We've found the spot to insert the new block
                tracing::debug!("current is beyond end of new block, inserting here");
                break;
            } else if current.end() < base {
                // This block is before the new block, so we can skip it
                tracing::debug!("current ends before new block starts, skipping");
                idx += 1;
                continue;
            }

            if current.base > base {
                // This block overlaps the new block, so we need to split it
                // The current block starts before the new block.
                if current.flags != new_block.flags {
                    tracing::warn!("overlapping blocks have differing flags",);
                }
                new_region_count += 1;
                if capacity_verified {
                    if merge_start == None {
                        merge_start = Some(idx);
                    }
                    merge_end = Some(idx + 1);

                    let block = MemoryBlock {
                        base,
                        size: current.base - base,
                        flags: new_block.flags,
                    };
                    self.insert_at(idx, block);

                    // We can skip the next block, since it's the one we just considered
                    idx += 1;
                }
            }

            // We no longer need to consider the portion of this block that overlapped.
            base = Ord::min(current.end(), new_block.end());
            tracing::debug!(
                base = ?base, "updated base");
            idx += 1;
        }

        // After we've seen every block we need to, we can insert the new block here, if we have any block left to insert.
        if base < new_block.end() {
            tracing::debug!("inserting final segment");
            new_region_count += 1;
            if capacity_verified {
                if merge_start == None {
                    merge_start = Some(idx);
                }
                merge_end = Some(idx + 1);

                let block = MemoryBlock {
                    base,
                    size: new_block.end() - base,
                    flags: new_block.flags,
                };
                self.insert_at(idx, block);
            }
        }

        if new_region_count == 0 {
            return Ok(());
        }

        if !capacity_verified {
            // Check if we have enough space to insert the new blocks
            if self.len + new_region_count > self.cap {
                // We don't have enough space to insert the new blocks.
                // Report the capacity needed and return.
                return Err(self.len + new_region_count);
            }

            // We have enough space, so just try again with capacity_verified set to true.
            self.try_insert(new_block, true)
        } else {
            // We did the insertions, because we had enough capacity.
            // Now we need to merge any blocks that we split.
            self.merge(merge_start, merge_end);
            Ok(())
        }
    }

    /// Tries to insert a block at the given index.
    /// This function assumes that the calling code has already asserted that the block can be inserted at this index
    /// without causing an overlap or the list to become unsorted.
    /// However, this function will still fail if the list is at capacity, returning a value of 1.
    fn insert_at(&mut self, index: usize, block: MemoryBlock) {
        assert!(self.len < self.cap, "list is at capacity");
        tracing::debug!(index = index, block = ?block, "inserting block");

        // Move all the blocks after this one up by one
        let current = unsafe { self.ptr.add(index) };
        let count = self.len - index;
        let dest = unsafe { current.add(1) };
        unsafe {
            current.copy_to(dest, count);
            current.write(block);
        }
        self.len += 1;
    }

    #[tracing::instrument(level = "debug", skip(self))]
    fn merge(&mut self, merge_start: Option<usize>, merge_end: Option<usize>) {
        let mut merge_end = Ord::min(self.len - 1, merge_end.unwrap_or(self.len - 1));
        let mut i = merge_start.unwrap_or_default();
        if i > 0 {
            // We need to start at the block before the first block we're merging
            i -= 1;
        }

        while i < merge_end {
            let current = unsafe { self.ptr.add(i).as_mut().unwrap() };
            let next = unsafe { self.ptr.add(i + 1).as_mut().unwrap() };

            if current.end() == next.base && current.flags == next.flags {
                tracing::debug!(current = ?current, next = ?next, "merging blocks");
                // We can merge these two blocks
                current.size += next.size;

                // Now slide everything down to clobber next, which we just merged into current
                let dest = unsafe { self.ptr.add(i + 1) };
                let src = unsafe { self.ptr.add(i + 2) };
                let count = self.len - (i + 2);
                unsafe {
                    src.copy_to(dest, count);
                }

                // Reduce the length AND the merge_end, since we just removed a block
                self.len -= 1;
                merge_end -= 1;

                // Don't increment i, we now need to consider the merged block against the next block
            } else {
                tracing::debug!(current = ?current, next = ?next, "cannot merge blocks");
                // Assert these are non-overlapping regions.
                assert!(current.end() <= next.base);
                i += 1;
            }
        }
    }
}

pub struct BlockAllocator {
    memory_blocks: MemoryBlockList,
    reserved_blocks: MemoryBlockList,
}

impl BlockAllocator {
    /// Initializes a new `BlockAllocator` with the given architecture.
    ///
    /// # Safety
    unsafe fn new(
        initial_memory_blocks: &mut [MaybeUninit<MemoryBlock>; 32],
        initial_reserved_blocks: &mut [MaybeUninit<MemoryBlock>; 32],
    ) -> Self {
        unsafe {
            // SAFETY: This is single-threaded and we are the only users of the memory blocks.
            BlockAllocator {
                memory_blocks: MemoryBlockList::from_array(initial_memory_blocks),
                reserved_blocks: MemoryBlockList::from_array(initial_reserved_blocks),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use core::mem::MaybeUninit;
    use test_log::test;

    use crate::{
        PhysicalAddress,
        allocator::block::{MemoryBlock, MemoryBlockFlags, MemoryBlockList},
    };

    #[test]
    pub fn insert_into_empty_list() {
        static mut BLOCKS: [MaybeUninit<MemoryBlock>; 4] = [const { MaybeUninit::uninit() }; 4];
        let mut block_list = unsafe { MemoryBlockList::from_array(&raw mut BLOCKS) };
        assert_eq!(4, block_list.capacity());

        let new_block = MemoryBlock {
            base: PhysicalAddress::new(0),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(new_block.clone(), true).unwrap();
        assert_eq!(std::vec![new_block], block_list.into_vec());
    }

    #[test]
    pub fn insert_non_overlapping_non_adjacent_leading() {
        static mut BLOCKS: [MaybeUninit<MemoryBlock>; 4] = [const { MaybeUninit::uninit() }; 4];
        let mut block_list = unsafe { MemoryBlockList::from_array(&raw mut BLOCKS) };
        assert_eq!(4, block_list.capacity());

        let right_block = MemoryBlock {
            base: PhysicalAddress::new(0x2000),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(right_block.clone(), true).unwrap();
        let new_block = MemoryBlock {
            base: PhysicalAddress::new(0),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(new_block.clone(), true).unwrap();
        assert_eq!(std::vec![new_block, right_block], block_list.into_vec());
    }

    #[test]
    pub fn insert_non_overlapping_non_adjacent_trailing() {
        static mut BLOCKS: [MaybeUninit<MemoryBlock>; 4] = [const { MaybeUninit::uninit() }; 4];
        let mut block_list = unsafe { MemoryBlockList::from_array(&raw mut BLOCKS) };
        assert_eq!(4, block_list.capacity());

        let left_block = MemoryBlock {
            base: PhysicalAddress::new(0x2000),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(left_block.clone(), true).unwrap();
        let new_block = MemoryBlock {
            base: PhysicalAddress::new(0x4000),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(new_block.clone(), true).unwrap();
        assert_eq!(std::vec![left_block, new_block], block_list.into_vec());
    }

    #[test]
    pub fn insert_non_overlapping_adjacent_leading_same_flags() {
        static mut BLOCKS: [MaybeUninit<MemoryBlock>; 4] = [const { MaybeUninit::uninit() }; 4];
        let mut block_list = unsafe { MemoryBlockList::from_array(&raw mut BLOCKS) };
        assert_eq!(4, block_list.capacity());

        let right_block = MemoryBlock {
            base: PhysicalAddress::new(0x2000),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(right_block.clone(), true).unwrap();
        let new_block = MemoryBlock {
            base: PhysicalAddress::new(0x1000),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(new_block.clone(), true).unwrap();
        assert_eq!(
            std::vec![MemoryBlock {
                base: PhysicalAddress::new(0x1000),
                size: 0x2000,
                flags: MemoryBlockFlags::empty(),
            }],
            block_list.into_vec()
        );
    }

    #[test]
    pub fn insert_non_overlapping_adjacent_leading_different_flags() {
        static mut BLOCKS: [MaybeUninit<MemoryBlock>; 4] = [const { MaybeUninit::uninit() }; 4];
        let mut block_list = unsafe { MemoryBlockList::from_array(&raw mut BLOCKS) };
        assert_eq!(4, block_list.capacity());

        let right_block = MemoryBlock {
            base: PhysicalAddress::new(0x2000),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(right_block.clone(), true).unwrap();
        let new_block = MemoryBlock {
            base: PhysicalAddress::new(0x1000),
            size: 0x1000,
            flags: MemoryBlockFlags::TEST,
        };
        block_list.try_insert(new_block.clone(), true).unwrap();
        assert_eq!(std::vec![new_block, right_block], block_list.into_vec());
    }

    #[test]
    pub fn insert_non_overlapping_adjacent_trailing_same_flags() {
        static mut BLOCKS: [MaybeUninit<MemoryBlock>; 4] = [const { MaybeUninit::uninit() }; 4];
        let mut block_list = unsafe { MemoryBlockList::from_array(&raw mut BLOCKS) };
        assert_eq!(4, block_list.capacity());

        let left_block = MemoryBlock {
            base: PhysicalAddress::new(0),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(left_block.clone(), true).unwrap();
        let right_block = MemoryBlock {
            base: PhysicalAddress::new(0x4000),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(right_block.clone(), true).unwrap();
        let new_block = MemoryBlock {
            base: PhysicalAddress::new(0x1000),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(new_block.clone(), true).unwrap();
        assert_eq!(
            std::vec![
                MemoryBlock {
                    base: PhysicalAddress::new(0),
                    size: 0x2000,
                    flags: MemoryBlockFlags::empty(),
                },
                right_block
            ],
            block_list.into_vec()
        );
    }

    #[test]
    pub fn insert_non_overlapping_adjacent_trailing_different_flags() {
        static mut BLOCKS: [MaybeUninit<MemoryBlock>; 4] = [const { MaybeUninit::uninit() }; 4];
        let mut block_list = unsafe { MemoryBlockList::from_array(&raw mut BLOCKS) };
        assert_eq!(4, block_list.capacity());

        let left_block = MemoryBlock {
            base: PhysicalAddress::new(0),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(left_block.clone(), true).unwrap();
        let right_block = MemoryBlock {
            base: PhysicalAddress::new(0x4000),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(right_block.clone(), true).unwrap();
        let new_block = MemoryBlock {
            base: PhysicalAddress::new(0x1000),
            size: 0x1000,
            flags: MemoryBlockFlags::TEST,
        };
        block_list.try_insert(new_block.clone(), true).unwrap();
        assert_eq!(
            std::vec![
                // TODO: Merging
                left_block,
                new_block,
                right_block
            ],
            block_list.into_vec()
        );
    }

    #[test]
    pub fn insert_overlapping_leading() {
        static mut BLOCKS: [MaybeUninit<MemoryBlock>; 4] = [const { MaybeUninit::uninit() }; 4];
        let mut block_list = unsafe { MemoryBlockList::from_array(&raw mut BLOCKS) };
        assert_eq!(4, block_list.capacity());

        let current_block = MemoryBlock {
            base: PhysicalAddress::new(0x1000),
            size: 0x4000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(current_block.clone(), true).unwrap();
        let overlapping_block = MemoryBlock {
            base: PhysicalAddress::new(0),
            size: 0x2000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list
            .try_insert(overlapping_block.clone(), true)
            .unwrap();
        assert_eq!(
            std::vec![MemoryBlock {
                base: PhysicalAddress::new(0),
                size: 0x5000,
                flags: MemoryBlockFlags::empty(),
            },],
            block_list.into_vec()
        );
    }
}
