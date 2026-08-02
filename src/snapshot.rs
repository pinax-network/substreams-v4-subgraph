//! Serializable boundary between deterministic replay and Parquet materialization.

use crate::{entities::EntityRecord, state::EntityState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedVersion {
    pub vid: i64,
    pub block_range_start: i32,
    pub entity: EntityRecord,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoiVersion {
    pub vid: i64,
    pub block_range_start: i32,
    pub id: String,
    pub digest: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplaySnapshot {
    pub state: EntityState,
    pub seed_versions: Vec<SeedVersion>,
    pub table_max_vids: BTreeMap<String, i64>,
    pub poi_seed: Option<PoiVersion>,
}
