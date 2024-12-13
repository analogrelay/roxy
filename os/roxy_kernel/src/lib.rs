#![no_std]
#![feature(abi_x86_interrupt)]
#![cfg_attr(test, feature(test))]

#[cfg(test)]
extern crate std;

#[cfg(test)]
extern crate test;

// Just don't use it until the heap is active!
extern crate alloc;

pub mod boot;
pub mod heap;
pub mod vmm;
