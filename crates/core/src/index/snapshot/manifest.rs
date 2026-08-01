use serde::{Deserialize, Serialize};

use super::identity::SnapshotId;
use crate::index::record::IndexRecord;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotManifest {
    pub id: SnapshotId,
    pub indexes: Vec<IndexRecord>,
}
