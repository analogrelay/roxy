#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

// Just don't use it until the heap is active!
extern crate alloc;

mod boot;
mod heap;
mod vmm;
