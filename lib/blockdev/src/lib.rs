#![feature(decl_macro)]
#![cfg_attr(feature = "no_std", no_std)]

#[macro_use]
extern crate alloc;

pub mod block_device;
pub mod mount;
