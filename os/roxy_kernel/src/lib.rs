#![no_std]
#![feature(abi_x86_interrupt)]
#![feature(allocator_api)]
#![cfg_attr(test, feature(test))]

#[cfg(test)]
extern crate std;

#[cfg(test)]
extern crate test;

// Just don't use it until the heap is active!
extern crate alloc;

pub mod arch;
mod framebuffer;
mod heap;
mod logger;
mod serial;
mod utils;
mod vmm;
