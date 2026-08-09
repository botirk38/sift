use crate::StoreMeta;
use crate::corpus::filter::FileFilter;
use crate::index::Indexes;

use super::scope::ScanScope;

/// Indexes, filters, and metadata used to resolve candidate files.
pub struct Scan<'a> {
    pub indexes: Option<&'a Indexes>,
    pub filter: &'a FileFilter,
    pub store_meta: Option<&'a StoreMeta>,
    pub scope: ScanScope,
}

impl<'a> Scan<'a> {
    #[must_use]
    pub const fn new(
        indexes: Option<&'a Indexes>,
        filter: &'a FileFilter,
        store_meta: Option<&'a StoreMeta>,
        scope: ScanScope,
    ) -> Self {
        Self {
            indexes,
            filter,
            store_meta,
            scope,
        }
    }
}
