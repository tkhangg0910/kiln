//! Everything `unsafe` in the crate: font mapping and the Win32 calls, behind safe functions.
#![allow(unsafe_code)]

pub mod mmap;
#[cfg(windows)]
pub mod win;
