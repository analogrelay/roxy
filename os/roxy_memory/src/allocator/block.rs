use core::{
    arch,
    cell::Ref,
    fmt::Debug,
    mem::MaybeUninit,
    ops::{Bound, Range, RangeBounds},
    ptr::NonNull,
    usize,
};

use crate::{Architecture, Error, PhysicalAddress, VirtualAddress};

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

    // Primarily for test purposes
    pub unsafe fn from_initialized_array<const SIZE: usize>(
        array: *mut [MemoryBlock; SIZE],
    ) -> Self {
        let ptr = array as *mut MemoryBlock;
        Self {
            ptr,
            len: SIZE,
            cap: SIZE,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn capacity(&self) -> usize {
        self.cap
    }

    pub fn iter(&self) -> impl Iterator<Item = MemoryBlock> {
        MemoryBlockIter { list: self, idx: 0 }
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

    /// Returns a slice of the memory blocks.
    pub fn as_slice(&self) -> &[MemoryBlock] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
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

struct MemoryBlockIter<'a> {
    list: &'a MemoryBlockList,
    idx: usize,
}

impl<'a> Iterator for MemoryBlockIter<'a> {
    type Item = MemoryBlock;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.list.len {
            return None;
        }
        let block = unsafe { self.list.ptr.add(self.idx).read() };
        self.idx += 1;
        Some(block)
    }
}

struct FreeBlockIterator<'a> {
    free_blocks: &'a [MemoryBlock],
    free_index: usize,
    active_free_block: Option<MemoryBlock>,

    reserved_blocks: &'a [MemoryBlock],
    reserved_index: usize,
    active_reserved_block: Option<MemoryBlock>,
}

impl<'a> FreeBlockIterator<'a> {
    pub fn new(free_blocks: &'a [MemoryBlock], reserved_blocks: &'a [MemoryBlock]) -> Self {
        Self {
            free_blocks,
            free_index: 0,
            active_free_block: free_blocks.get(0).cloned(),
            reserved_blocks,
            reserved_index: 0,
            active_reserved_block: reserved_blocks.get(0).cloned(),
        }
    }

    fn advance_free(&mut self) {
        self.free_index += 1;
        self.active_free_block = self.free_blocks.get(self.free_index).cloned();
    }

    fn advance_reserved(&mut self) {
        self.reserved_index += 1;
        self.active_reserved_block = self.reserved_blocks.get(self.reserved_index).cloned();
    }
}

impl<'a> Iterator for FreeBlockIterator<'a> {
    type Item = MemoryBlock;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(active_block) = self.active_free_block.clone() {
                if let Some(reserved) = self.active_reserved_block.clone() {
                    if reserved.base >= active_block.end() {
                        // The free block is entirely before the reserved block, so just return it.
                        self.advance_free();
                        return Some(active_block);
                    } else if active_block.base > reserved.end() {
                        // The free block is entirely after the reserved block, so move to the next reserved block.
                        self.advance_reserved();
                        // We can't return anything yet though, there might be a reserved block later that overlaps the free block.
                    } else {
                        // The blocks overlap, so we need to split the free block up.
                        // Take the portion of the free block that is before the reserved block.
                        let block = MemoryBlock {
                            base: active_block.base,
                            size: reserved.base - active_block.base,
                            flags: active_block.flags,
                        };

                        // Calculate the new ending, which is the minimum of the reserved block's end and the active block's end.
                        let new_end = Ord::min(reserved.end(), active_block.end());

                        // Update the active free block, if there is any remaining.
                        if new_end < active_block.end() {
                            // There is some remaining, so update the active free block to reflect that.
                            self.active_free_block = Some(MemoryBlock {
                                base: new_end,
                                size: active_block.end() - new_end,
                                flags: active_block.flags,
                            });
                        } else {
                            self.advance_free();
                        }

                        // Update the active reserved block, if there is any remaining.
                        if new_end < reserved.end() {
                            // There is some remaining, so update the active free block to reflect that.
                            self.active_reserved_block = Some(MemoryBlock {
                                base: new_end,
                                size: reserved.end() - new_end,
                                flags: reserved.flags,
                            });
                        } else {
                            self.advance_reserved();
                        }

                        // The block we carved off may be empty, if it is, don't return anything.
                        if block.size > 0 {
                            return Some(block);
                        }
                    }
                } else {
                    // No reserved blocks, so we can just return the free block and move on
                    self.advance_free();
                    return Some(active_block);
                }
            } else {
                // No more free blocks, so we're done.
                return None;
            }
        }
    }
}

pub struct BlockAllocator<'a, A> {
    arch: &'a A,
    memory_blocks: MemoryBlockList,
    reserved_blocks: MemoryBlockList,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MemoryBlockType {
    Memory,
    Reserved,
}

impl<'a, A: Architecture> BlockAllocator<'a, A> {
    /// Initializes a new `BlockAllocator` with the given architecture.
    ///
    /// # Safety
    unsafe fn new<const SIZE: usize>(
        arch: &'a A,
        initial_memory_blocks: &mut [MaybeUninit<MemoryBlock>; SIZE],
        initial_reserved_blocks: &mut [MaybeUninit<MemoryBlock>; SIZE],
    ) -> Self {
        unsafe {
            // SAFETY: This is single-threaded and we are the only users of the memory blocks.
            Self {
                arch,
                memory_blocks: MemoryBlockList::from_array(initial_memory_blocks),
                reserved_blocks: MemoryBlockList::from_array(initial_reserved_blocks),
            }
        }
    }

    fn from_lists(
        arch: &'a A,
        memory_blocks: MemoryBlockList,
        reserved_blocks: MemoryBlockList,
    ) -> Self {
        Self {
            arch,
            memory_blocks,
            reserved_blocks,
        }
    }

    /// Returns an iterator over all the free areas in the allocator.
    pub fn free_areas(&self) -> impl Iterator<Item = MemoryBlock> {
        FreeBlockIterator::new(
            self.memory_blocks.as_slice(),
            self.reserved_blocks.as_slice(),
        )
    }

    /// Finds a free range of memory that fits the given size and alignment.
    ///
    /// IMPORTANT: This function does not reserve the region, it only finds a suitable range.
    fn find_free_range(
        &self,
        candidate_range: impl RangeBounds<PhysicalAddress>,
        size: usize,
        align: usize,
    ) -> Option<PhysicalAddress> {
        for area in self.free_areas() {
            let range_start = clamp_addr(area.base, &candidate_range);
            let range_end = clamp_addr(area.end(), &candidate_range);

            // Get the start of the block, aligned to the requested alignment
            let candidate = range_start.align_up(align);
            if candidate < range_end && range_end.value() >= size && range_end - size >= candidate {
                // We can fit the block here
                return Some(candidate);
            }
        }
        None
    }

    fn list(&self, list_type: MemoryBlockType) -> &MemoryBlockList {
        match list_type {
            MemoryBlockType::Memory => &self.memory_blocks,
            MemoryBlockType::Reserved => &self.reserved_blocks,
        }
    }

    fn list_mut(&mut self, list_type: MemoryBlockType) -> &mut MemoryBlockList {
        match list_type {
            MemoryBlockType::Memory => &mut self.memory_blocks,
            MemoryBlockType::Reserved => &mut self.reserved_blocks,
        }
    }

    fn add_block(&mut self, list_type: MemoryBlockType, block: MemoryBlock) -> Result<(), Error> {
        let result = self.list_mut(list_type).try_insert(block.clone(), false);
        if let Err(capacity_needed) = result {
            let target = capacity_needed.next_power_of_two();

            // If we're adding to the reserved list, we need the resize to avoid the new block we're about to add
            // Because reservations are added _AFTER_ the memory is in use, and if we expand in to that block,
            // we'll clobber whatever is there.
            let resize_avoid = match list_type {
                MemoryBlockType::Reserved => Some(block.base..block.end()),
                _ => None,
            };
            self.expand_list(list_type, target, resize_avoid)?;
            self.list_mut(list_type).try_insert(block, true).unwrap();
        }
        Ok(())
    }

    fn expand_list(
        &mut self,
        list_type: MemoryBlockType,
        new_capacity: usize,
        resize_avoid: Option<Range<PhysicalAddress>>,
    ) -> Result<(), Error> {
        // Find space for the new list
        let align = align_of::<MemoryBlock>();
        let size = new_capacity * size_of::<MemoryBlock>();
        let space = match resize_avoid {
            None => self.find_free_range(.., size, align),
            Some(range) => self
                .find_free_range(..range.start, size, align)
                .or_else(|| self.find_free_range(range.end.., size, align)),
        };
        let space = space.ok_or(Error::OutOfPhysicalMemory)?;
        let space = self.arch.virtual_address_for(space);

        // Copy the old list to the new space
        let old_list = self.list(list_type);
        unsafe {
            self.arch.copy(
                old_list.ptr.into(),
                space,
                old_list.len() * size_of::<MemoryBlock>(),
            )
        };

        // And update the list
        let old_list = self.list_mut(list_type);
        old_list.ptr = space.as_mut_ptr();
        old_list.cap = new_capacity;
        Ok(())
    }
}

fn clamp_addr(
    value: PhysicalAddress,
    range: &impl RangeBounds<PhysicalAddress>,
) -> PhysicalAddress {
    let min_clamped = match range.start_bound() {
        Bound::Excluded(min) if value <= *min => *min + 1,
        Bound::Included(min) if value < *min => *min,
        _ => value,
    };
    match range.end_bound() {
        Bound::Excluded(max) if min_clamped >= *max => *max - 1,
        Bound::Included(max) if min_clamped > *max => *max,
        _ => min_clamped,
    }
}

#[cfg(test)]
mod tests {
    use core::mem::MaybeUninit;
    use std::vec::Vec;
    use test_log::test;

    use crate::{
        Architecture, Emulated, EmulatedMachine, PhysicalAddress, X8664,
        allocator::block::{
            BlockAllocator, FreeBlockIterator, MemoryBlock, MemoryBlockFlags, MemoryBlockList,
            MemoryBlockType,
        },
    };

    #[test]
    pub fn expand_list() {
        const SIZE: usize = size_of::<[MemoryBlock; 4]>();
        assert_eq!(0x60, SIZE);
        let (machine, areas) = EmulatedMachine::<X8664>::new(0x10_000);
        let mut arch = Emulated::new(X8664, machine);

        let mut ptr = areas[0].base;

        // Place the arrays at the start of memory
        let memory_start = arch.virtual_address_for(ptr);
        ptr += SIZE;

        let reserved_start = arch.virtual_address_for(ptr);
        ptr += SIZE;

        let (memory_ptr, reserved_ptr) = {
            let mut machine = arch.machine_mut();
            (
                machine.get_memory(memory_start, SIZE).unwrap()
                    as *mut [MaybeUninit<MemoryBlock>; 4],
                machine.get_memory(reserved_start, SIZE).unwrap()
                    as *mut [MaybeUninit<MemoryBlock>; 4],
            )
        };

        let mut allocator = unsafe {
            BlockAllocator::new(
                &arch,
                &mut *(memory_ptr as *mut [MaybeUninit<MemoryBlock>; 4]),
                &mut *(reserved_ptr as *mut [MaybeUninit<MemoryBlock>; 4]),
            )
        };

        // Add areas
        assert_eq!(1, areas.len());
        assert_eq!(PhysicalAddress::new(0x4000), areas[0].base);
        assert_eq!(PhysicalAddress::new(0x10_000), areas[0].end());
        allocator
            .add_block(
                MemoryBlockType::Memory,
                // 0x00_000 - 0x10_000 is our memory.
                MemoryBlock {
                    base: PhysicalAddress::new(0),
                    size: 0x10_000,
                    flags: MemoryBlockFlags::empty(),
                },
            )
            .unwrap();
        allocator
            .add_block(
                MemoryBlockType::Reserved,
                // 0x00_000 - 0x04_000
                MemoryBlock {
                    base: PhysicalAddress::new(0),
                    size: 0x4000,
                    flags: MemoryBlockFlags::empty(),
                },
            )
            .unwrap();
        assert_eq!(1, allocator.list(MemoryBlockType::Reserved).len());

        allocator
            .add_block(MemoryBlockType::Reserved, MemoryBlock {
                base: PhysicalAddress::new(0x4000),
                size: 0xC0,
                flags: MemoryBlockFlags::TEST, // Alternate flags to prevent merging
            })
            .unwrap();
        assert_eq!(2, allocator.list(MemoryBlockType::Reserved).len());

        // Reserve Q - Q + 0x500 and Q + 0x500 to Q + 0x1000 to force expansion to move beyond that
        allocator
            .add_block(MemoryBlockType::Reserved, MemoryBlock {
                base: PhysicalAddress::new(0x40C0),
                size: 0x500,
                flags: MemoryBlockFlags::empty(),
            })
            .unwrap();
        assert_eq!(3, allocator.list(MemoryBlockType::Reserved).len());
        allocator
            .add_block(MemoryBlockType::Reserved, MemoryBlock {
                base: PhysicalAddress::new(0x45C0),
                size: 0x500,
                flags: MemoryBlockFlags::TEST,
            })
            .unwrap();
        assert_eq!(4, allocator.list(MemoryBlockType::Reserved).len());

        assert_eq!(
            std::vec![MemoryBlock {
                base: PhysicalAddress::new(0x4AC0),
                size: 0xB540,
                flags: MemoryBlockFlags::empty(),
            }],
            allocator.free_areas().collect::<Vec<_>>()
        );

        // Now, simulate reserving Q + 0x1000 - Q + 0x2000, which would force expansion AND
        // we'll have to avoid the new reserved block.
        allocator
            .expand_list(
                MemoryBlockType::Reserved,
                8,
                Some(PhysicalAddress::new(0x4AD0)..PhysicalAddress::new(0x5000)),
            )
            .unwrap();

        assert_eq!(4, allocator.list(MemoryBlockType::Memory).capacity());
        assert_eq!(8, allocator.list(MemoryBlockType::Reserved).capacity());

        // Verify that the new list was placed correctly
        let new_reserved = allocator.list(MemoryBlockType::Reserved).ptr as usize;
        assert_eq!(0xFFFF800000005000, new_reserved);
    }

    fn create_find_range_allocator<'a, A: Architecture>(arch: &'a A) -> BlockAllocator<'a, A> {
        static mut FREE: [MemoryBlock; 2] = [
            MemoryBlock {
                // 0x00_000 - 0x10_000
                base: PhysicalAddress::new(0),
                size: 0x10_000,
                flags: MemoryBlockFlags::empty(),
            },
            MemoryBlock {
                // 0x10_000 - 0x20_000
                base: PhysicalAddress::new(0x10_000),
                size: 0x10_000,
                flags: MemoryBlockFlags::empty(),
            },
        ];
        let memory_blocks = unsafe { MemoryBlockList::from_initialized_array(&raw mut FREE) };
        static mut RESERVED: [MemoryBlock; 2] = [
            // 0x01_000 - 0x04_000
            MemoryBlock {
                base: PhysicalAddress::new(0x1_000),
                size: 0x3_000,
                flags: MemoryBlockFlags::empty(),
            },
            // 0x10_000 - 0x18_000
            MemoryBlock {
                base: PhysicalAddress::new(0x10_000),
                size: 0x8_000,
                flags: MemoryBlockFlags::empty(),
            },
        ];
        let reserved_blocks = unsafe { MemoryBlockList::from_initialized_array(&raw mut RESERVED) };
        let allocator = BlockAllocator::from_lists(arch, memory_blocks, reserved_blocks);
        assert_eq!(
            std::vec![
                // 0x00_000 - 0x01_000
                MemoryBlock {
                    base: PhysicalAddress::new(0),
                    size: 0x1000,
                    flags: MemoryBlockFlags::empty(),
                },
                // 0x04_000 - 0x10_000
                MemoryBlock {
                    base: PhysicalAddress::new(0x4000),
                    size: 0xC000,
                    flags: MemoryBlockFlags::empty(),
                },
                // 0x18_000 - 0x20_000
                MemoryBlock {
                    base: PhysicalAddress::new(0x18_000),
                    size: 0x8000,
                    flags: MemoryBlockFlags::empty(),
                },
            ],
            allocator.free_areas().collect::<Vec<_>>()
        );
        allocator
    }

    #[test]
    pub fn find_range_unrestricted() {
        let allocator = create_find_range_allocator(&X8664);
        assert_eq!(
            Some(PhysicalAddress::new(0x0000)),
            allocator.find_free_range(.., 0x1000, 0x1000)
        );
        assert_eq!(
            Some(PhysicalAddress::new(0x4000)),
            allocator.find_free_range(.., 0x8000, 0x1000)
        );
        assert_eq!(
            Some(PhysicalAddress::new(0x6000)),
            // Weird alignment, but it should still work
            allocator.find_free_range(.., 0x2000, 0x3000)
        );
    }

    #[test]
    pub fn find_range_restricted_lower() {
        let allocator = create_find_range_allocator(&X8664);
        assert_eq!(
            Some(PhysicalAddress::new(0x6000)),
            allocator.find_free_range(PhysicalAddress::new(0x6000).., 0x1000, 0x1000)
        );
        assert_eq!(
            Some(PhysicalAddress::new(0x18_000)),
            allocator.find_free_range(PhysicalAddress::new(0x17_000).., 0x8_000, 0x1000)
        );
    }

    #[test]
    pub fn find_range_restricted_upper() {
        let allocator = create_find_range_allocator(&X8664);
        assert_eq!(
            None,
            // Plenty of ranges above 0x2000 can accomodate this, but we're restricting the upper bound.
            allocator.find_free_range(..PhysicalAddress::new(0x2000), 0x2000, 0x1000)
        );
    }

    #[test]
    pub fn find_range_restricted_full() {
        let allocator = create_find_range_allocator(&X8664);
        assert_eq!(
            Some(PhysicalAddress::new(0x18_000)),
            allocator.find_free_range(
                PhysicalAddress::new(0x14_000)..PhysicalAddress::new(0x20_000),
                0x2000,
                0x1000
            )
        );
    }

    #[test]
    pub fn free_areas_returns_available_areas() {
        static FREE: [MemoryBlock; 4] = [
            MemoryBlock {
                base: PhysicalAddress::new(0),
                size: 0x1000,
                flags: MemoryBlockFlags::empty(),
            },
            MemoryBlock {
                base: PhysicalAddress::new(0x2000),
                size: 0x1000,
                flags: MemoryBlockFlags::empty(),
            },
            MemoryBlock {
                base: PhysicalAddress::new(0x4000),
                size: 0x1000,
                flags: MemoryBlockFlags::empty(),
            },
            MemoryBlock {
                base: PhysicalAddress::new(0x6000),
                size: 0x1000,
                flags: MemoryBlockFlags::empty(),
            },
        ];
        static RESERVED: [MemoryBlock; 0] = [];
        let areas = FreeBlockIterator::new(&FREE, &RESERVED).collect::<Vec<_>>();
        assert_eq!(FREE.to_vec(), areas);
    }

    #[test]
    pub fn free_areas_excludes_reserved_areas() {
        static FREE: [MemoryBlock; 4] = [
            MemoryBlock {
                base: PhysicalAddress::new(0),
                size: 0x1000,
                flags: MemoryBlockFlags::empty(),
            },
            MemoryBlock {
                base: PhysicalAddress::new(0x2000),
                size: 0x1000,
                flags: MemoryBlockFlags::empty(),
            },
            MemoryBlock {
                base: PhysicalAddress::new(0x4000),
                size: 0x1000,
                flags: MemoryBlockFlags::empty(),
            },
            MemoryBlock {
                base: PhysicalAddress::new(0x6000),
                size: 0x1000,
                flags: MemoryBlockFlags::empty(),
            },
        ];
        static RESERVED: [MemoryBlock; 2] = [
            MemoryBlock {
                base: PhysicalAddress::new(0x2000),
                size: 0x1000,
                flags: MemoryBlockFlags::empty(),
            },
            MemoryBlock {
                base: PhysicalAddress::new(0x6000),
                size: 0x1000,
                flags: MemoryBlockFlags::empty(),
            },
        ];
        let areas = FreeBlockIterator::new(&FREE, &RESERVED).collect::<Vec<_>>();
        assert_eq!(std::vec![FREE[0].clone(), FREE[2].clone(),], areas);
    }

    #[test]
    pub fn free_areas_splits_areas_if_middle_reserved() {
        static FREE: [MemoryBlock; 2] = [
            MemoryBlock {
                base: PhysicalAddress::new(0),
                size: 0x1000,
                flags: MemoryBlockFlags::empty(),
            },
            MemoryBlock {
                base: PhysicalAddress::new(0x2000),
                size: 0x4000,
                flags: MemoryBlockFlags::empty(),
            },
        ];
        static RESERVED: [MemoryBlock; 2] = [
            MemoryBlock {
                base: PhysicalAddress::new(0x3000),
                size: 0x1000,
                flags: MemoryBlockFlags::empty(),
            },
            MemoryBlock {
                base: PhysicalAddress::new(0x5000),
                size: 0x2000,
                flags: MemoryBlockFlags::empty(),
            },
        ];
        let areas = FreeBlockIterator::new(&FREE, &RESERVED).collect::<Vec<_>>();
        assert_eq!(
            std::vec![
                MemoryBlock {
                    base: PhysicalAddress::new(0),
                    size: 0x1000,
                    flags: MemoryBlockFlags::empty(),
                },
                MemoryBlock {
                    base: PhysicalAddress::new(0x2000),
                    size: 0x1000,
                    flags: MemoryBlockFlags::empty(),
                },
                MemoryBlock {
                    base: PhysicalAddress::new(0x4000),
                    size: 0x1000,
                    flags: MemoryBlockFlags::empty(),
                },
            ],
            areas
        );
    }

    #[test]
    pub fn insert_exceeding_capacity() {
        static mut BLOCKS: [MaybeUninit<MemoryBlock>; 2] = [const { MaybeUninit::uninit() }; 2];
        let mut block_list = unsafe { MemoryBlockList::from_array(&raw mut BLOCKS) };
        assert_eq!(2, block_list.capacity());

        let new_block = MemoryBlock {
            base: PhysicalAddress::new(0x1000),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(new_block.clone(), true).unwrap();
        let new_block = MemoryBlock {
            base: PhysicalAddress::new(0x3000),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(new_block.clone(), true).unwrap();
        let new_block = MemoryBlock {
            base: PhysicalAddress::new(0),
            size: 0x5000,
            flags: MemoryBlockFlags::empty(),
        };
        let err = block_list.try_insert(new_block.clone(), false).unwrap_err();
        assert_eq!(5, err);
    }

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
        block_list.try_insert(right_block.clone(), false).unwrap();
        let new_block = MemoryBlock {
            base: PhysicalAddress::new(0),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(new_block.clone(), false).unwrap();
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
        block_list.try_insert(new_block.clone(), false).unwrap();
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
        block_list.try_insert(right_block.clone(), false).unwrap();
        let new_block = MemoryBlock {
            base: PhysicalAddress::new(0x1000),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(new_block.clone(), false).unwrap();
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
        block_list.try_insert(right_block.clone(), false).unwrap();
        let new_block = MemoryBlock {
            base: PhysicalAddress::new(0x1000),
            size: 0x1000,
            flags: MemoryBlockFlags::TEST,
        };
        block_list.try_insert(new_block.clone(), false).unwrap();
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
        block_list.try_insert(left_block.clone(), false).unwrap();
        let right_block = MemoryBlock {
            base: PhysicalAddress::new(0x4000),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(right_block.clone(), false).unwrap();
        let new_block = MemoryBlock {
            base: PhysicalAddress::new(0x1000),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(new_block.clone(), false).unwrap();
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
        block_list.try_insert(left_block.clone(), false).unwrap();
        let right_block = MemoryBlock {
            base: PhysicalAddress::new(0x4000),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(right_block.clone(), false).unwrap();
        let new_block = MemoryBlock {
            base: PhysicalAddress::new(0x1000),
            size: 0x1000,
            flags: MemoryBlockFlags::TEST,
        };
        block_list.try_insert(new_block.clone(), false).unwrap();
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
        block_list.try_insert(current_block.clone(), false).unwrap();
        let overlapping_block = MemoryBlock {
            base: PhysicalAddress::new(0),
            size: 0x2000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list
            .try_insert(overlapping_block.clone(), false)
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

    #[test]
    pub fn insert_overlapping_leading_different_flags() {
        static mut BLOCKS: [MaybeUninit<MemoryBlock>; 4] = [const { MaybeUninit::uninit() }; 4];
        let mut block_list = unsafe { MemoryBlockList::from_array(&raw mut BLOCKS) };
        assert_eq!(4, block_list.capacity());

        let current_block = MemoryBlock {
            base: PhysicalAddress::new(0x1000),
            size: 0x4000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(current_block.clone(), false).unwrap();
        let overlapping_block = MemoryBlock {
            base: PhysicalAddress::new(0),
            size: 0x2000,
            flags: MemoryBlockFlags::TEST,
        };
        block_list
            .try_insert(overlapping_block.clone(), false)
            .unwrap();
        assert_eq!(
            std::vec![
                MemoryBlock {
                    base: PhysicalAddress::new(0),
                    size: 0x1000,
                    flags: MemoryBlockFlags::TEST,
                },
                MemoryBlock {
                    base: PhysicalAddress::new(0x1000),
                    size: 0x4000,
                    flags: MemoryBlockFlags::empty(),
                },
            ],
            block_list.into_vec()
        );
    }

    #[test]
    pub fn insert_overlapping_trailing() {
        static mut BLOCKS: [MaybeUninit<MemoryBlock>; 4] = [const { MaybeUninit::uninit() }; 4];
        let mut block_list = unsafe { MemoryBlockList::from_array(&raw mut BLOCKS) };
        assert_eq!(4, block_list.capacity());

        let current_block = MemoryBlock {
            base: PhysicalAddress::new(0x1000),
            size: 0x4000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(current_block.clone(), false).unwrap();
        let overlapping_block = MemoryBlock {
            base: PhysicalAddress::new(0x4000),
            size: 0x2000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list
            .try_insert(overlapping_block.clone(), false)
            .unwrap();
        assert_eq!(
            std::vec![MemoryBlock {
                base: PhysicalAddress::new(0x1000),
                size: 0x5000,
                flags: MemoryBlockFlags::empty(),
            },],
            block_list.into_vec()
        );
    }

    #[test]
    pub fn insert_overlapping_trailing_different_flags() {
        static mut BLOCKS: [MaybeUninit<MemoryBlock>; 4] = [const { MaybeUninit::uninit() }; 4];
        let mut block_list = unsafe { MemoryBlockList::from_array(&raw mut BLOCKS) };
        assert_eq!(4, block_list.capacity());

        let current_block = MemoryBlock {
            base: PhysicalAddress::new(0x1000),
            size: 0x4000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(current_block.clone(), false).unwrap();
        let overlapping_block = MemoryBlock {
            base: PhysicalAddress::new(0x4000),
            size: 0x2000,
            flags: MemoryBlockFlags::TEST,
        };
        block_list
            .try_insert(overlapping_block.clone(), false)
            .unwrap();
        assert_eq!(
            std::vec![
                MemoryBlock {
                    base: PhysicalAddress::new(0x1000),
                    size: 0x4000,
                    flags: MemoryBlockFlags::empty(),
                },
                MemoryBlock {
                    base: PhysicalAddress::new(0x5000),
                    size: 0x1000,
                    flags: MemoryBlockFlags::TEST,
                },
            ],
            block_list.into_vec()
        );
    }

    #[test]
    pub fn insert_overlapping_middle_same_flags() {
        static mut BLOCKS: [MaybeUninit<MemoryBlock>; 4] = [const { MaybeUninit::uninit() }; 4];
        let mut block_list = unsafe { MemoryBlockList::from_array(&raw mut BLOCKS) };
        assert_eq!(4, block_list.capacity());

        let current_block = MemoryBlock {
            base: PhysicalAddress::new(0x1000),
            size: 0x4000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(current_block.clone(), false).unwrap();
        let overlapping_block = MemoryBlock {
            base: PhysicalAddress::new(0x2000),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list
            .try_insert(overlapping_block.clone(), false)
            .unwrap();
        assert_eq!(std::vec![current_block], block_list.into_vec());
    }

    #[test]
    pub fn insert_overlapping_middle_different_flags() {
        static mut BLOCKS: [MaybeUninit<MemoryBlock>; 4] = [const { MaybeUninit::uninit() }; 4];
        let mut block_list = unsafe { MemoryBlockList::from_array(&raw mut BLOCKS) };
        assert_eq!(4, block_list.capacity());

        let current_block = MemoryBlock {
            base: PhysicalAddress::new(0x1000),
            size: 0x4000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(current_block.clone(), false).unwrap();
        let overlapping_block = MemoryBlock {
            base: PhysicalAddress::new(0x2000),
            size: 0x1000,
            flags: MemoryBlockFlags::TEST,
        };
        block_list
            .try_insert(overlapping_block.clone(), false)
            .unwrap();

        // Differing flags in overlapping blocks DO NOT cause a split!
        assert_eq!(
            std::vec![MemoryBlock {
                base: PhysicalAddress::new(0x1000),
                size: 0x4000,
                flags: MemoryBlockFlags::empty(),
            },],
            block_list.into_vec()
        );
    }

    #[test]
    pub fn insert_spanning() {
        static mut BLOCKS: [MaybeUninit<MemoryBlock>; 8] = [const { MaybeUninit::uninit() }; 8];
        let mut block_list = unsafe { MemoryBlockList::from_array(&raw mut BLOCKS) };
        assert_eq!(8, block_list.capacity());

        let new_block = MemoryBlock {
            base: PhysicalAddress::new(0x1000),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(new_block.clone(), false).unwrap();
        let new_block = MemoryBlock {
            base: PhysicalAddress::new(0x3000),
            size: 0x1000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(new_block.clone(), false).unwrap();
        let new_block = MemoryBlock {
            base: PhysicalAddress::new(0),
            size: 0x5000,
            flags: MemoryBlockFlags::empty(),
        };
        block_list.try_insert(new_block.clone(), false).unwrap();
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
