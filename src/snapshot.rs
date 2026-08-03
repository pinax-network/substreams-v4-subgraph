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

impl ReplaySnapshot {
    /// Adds active seed rows needed by a later segment when they were untouched
    /// by this snapshot's own replay window. This is used only to build bounded
    /// segmented fixtures; a complete production graft seed already contains
    /// every active mutable row.
    pub fn supplement_missing_seeds(&mut self, other: &Self) -> Result<usize, String> {
        let mut keys = self
            .seed_versions
            .iter()
            .map(|seed| {
                (
                    seed.entity.entity_type().to_owned(),
                    seed.entity.id().to_owned(),
                )
            })
            .collect::<std::collections::BTreeSet<_>>();
        let mut added = 0;
        for seed in &other.seed_versions {
            let entity_type = seed.entity.entity_type().to_owned();
            let key = (entity_type.clone(), seed.entity.id().to_owned());
            if keys.contains(&key) || self.state.get(&entity_type, seed.entity.id()).is_some() {
                continue;
            }
            let max_vid = self.table_max_vids.get(&entity_type).copied().unwrap_or(-1);
            if seed.vid > max_vid {
                return Err(format!(
                    "supplemental {entity_type} {} has VID {}, after primary seed max {max_vid}",
                    seed.entity.id(),
                    seed.vid
                ));
            }
            self.state
                .insert_seed(seed.entity.clone())
                .map_err(|error| error.to_string())?;
            self.seed_versions.push(seed.clone());
            keys.insert(key);
            added += 1;
        }
        self.seed_versions.sort_by(|left, right| {
            (left.entity.entity_type(), left.vid).cmp(&(right.entity.entity_type(), right.vid))
        });
        Ok(added)
    }
}
