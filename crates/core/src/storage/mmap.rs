//! Minimal memory-map wrapper.
//!
//! `Mmap::map` requires `unsafe` to dereference the raw pointer from the OS.
//! This module contains that unsafety in one place with a documented invariant.
//!
//! # Safety
//!
//! `Mmap::map` creates a memory mapping from a file descriptor. The OS manages
//! the mapping and will not allow access beyond the file bounds. The returned
//! `Mmap` is valid for the lifetime of the file descriptor — which must not be
//! closed while the mapping is alive. We immediately drop the `File` after
//! creating the mapping, relying on the OS's reference-counted mapping to keep
//! the data accessible.

#![allow(unsafe_code)]

use memmap2::Mmap;
use std::fs::File;
use std::io;
use std::path::Path;

pub fn open_mmap(path: &Path) -> io::Result<Mmap> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file) };
    drop(file);
    mmap
}
