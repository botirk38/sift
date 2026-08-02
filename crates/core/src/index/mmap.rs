//! Single source of truth for memory-mapped file access.
//!
//! Every `unsafe` in `sift-core` lives here with a documented safety
//! invariant.  Other modules import [`mmap_open`] instead of duplicating
//! the call.

use std::path::Path;

use memmap2::Mmap;

/// Memory-map a file for read access.
///
/// # Errors
///
/// Returns an error if the file cannot be opened or mapped.
///
/// # Safety invariant
///
/// `Mmap::map` dereferences the raw OS mapping pointer. The OS manages
/// bounds and the mapping outlives the closed `File` handle via refcount.
pub fn mmap_open(path: &Path) -> std::io::Result<Mmap> {
    let file = std::fs::File::open(path)?;
    // SAFETY: see module docs — sole `unsafe` in sift-core.
    unsafe { Mmap::map(&file) }
}
