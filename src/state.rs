//! Deterministic Graph mapping state used after parallel Substreams extraction.

use crate::{entities::*, pb::pinax::uniswap::v4::base::v1 as pb, reducer};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityChange {
    pub block_number: u64,
    pub block_hash: Vec<u8>,
    pub trigger_order: u64,
    pub operation_index: u64,
    pub immutable: bool,
    pub entity: EntityRecord,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum StateError {
    #[error("event is missing {0}")]
    Missing(&'static str),
    #[error("invalid {field} integer `{value}`")]
    InvalidInteger { field: &'static str, value: String },
    #[error("required {entity} `{id}` does not exist")]
    RequiredEntity { entity: &'static str, id: String },
    #[error("Initialize metadata is missing for token {0}")]
    MissingMetadata(String),
    #[error("immutable {entity} `{id}` changed after its first write")]
    ImmutableConflict { entity: &'static str, id: String },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityState {
    /// Canonical blocks that ran at least one mapping handler. Graph Node
    /// advances proof-of-indexing even when a handler writes no entities.
    #[serde(default)]
    pub processed_blocks: BTreeMap<u64, Vec<u8>>,
    pub pool_managers: BTreeMap<String, PoolManager>,
    pub bundles: BTreeMap<String, Bundle>,
    pub tokens: BTreeMap<String, Token>,
    pub pools: BTreeMap<String, Pool>,
    pub ticks: BTreeMap<String, Tick>,
    pub transactions: BTreeMap<String, Transaction>,
    pub swaps: BTreeMap<String, Swap>,
    pub modify_liquidities: BTreeMap<String, ModifyLiquidity>,
    pub uniswap_day_data: BTreeMap<String, UniswapDayData>,
    pub pool_day_data: BTreeMap<String, PoolIntervalData>,
    pub pool_hour_data: BTreeMap<String, PoolIntervalData>,
    pub token_day_data: BTreeMap<String, TokenIntervalData>,
    pub token_hour_data: BTreeMap<String, TokenIntervalData>,
    pub positions: BTreeMap<String, Position>,
    pub subscriptions: BTreeMap<String, Subscribe>,
    pub unsubscriptions: BTreeMap<String, Unsubscribe>,
    pub transfers: BTreeMap<String, Transfer>,
    pub arrakis_hooks: BTreeMap<String, ArrakisHook>,
    pub changes: Vec<EntityChange>,
    next_operation_index: u64,
}

impl EntityState {
    pub fn apply(&mut self, events: &pb::Events) -> Result<(), StateError> {
        let mut ordered = events.events.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|event| {
            let log = event.log.as_ref();
            (
                log.map(|value| value.graph_node_trigger_order)
                    .unwrap_or_default(),
                log.map(|value| value.block_log_index).unwrap_or_default(),
            )
        });
        for event in ordered {
            let block = event.block.as_ref().ok_or(StateError::Missing("block"))?;
            self.processed_blocks
                .entry(block.number)
                .or_insert_with(|| block.hash.clone());
            reducer::apply_event(self, event)?;
        }
        Ok(())
    }

    /// Inserts a canonical graft checkpoint without manufacturing mapping writes.
    pub fn insert_seed(&mut self, entity: EntityRecord) -> Result<(), StateError> {
        self.insert(entity, false, None)
    }

    pub fn entity_count(&self) -> usize {
        self.pool_managers.len()
            + self.bundles.len()
            + self.tokens.len()
            + self.pools.len()
            + self.ticks.len()
            + self.transactions.len()
            + self.swaps.len()
            + self.modify_liquidities.len()
            + self.uniswap_day_data.len()
            + self.pool_day_data.len()
            + self.pool_hour_data.len()
            + self.token_day_data.len()
            + self.token_hour_data.len()
            + self.positions.len()
            + self.subscriptions.len()
            + self.unsubscriptions.len()
            + self.transfers.len()
            + self.arrakis_hooks.len()
    }

    /// Retains only mutable mapping state needed by the next contiguous replay
    /// segment. Historical changes and immutable rows already materialized to
    /// Parquet must not grow the resume checkpoint indefinitely.
    pub fn prepare_resume_checkpoint(&mut self) {
        self.processed_blocks.clear();
        self.transactions.clear();
        self.swaps.clear();
        self.modify_liquidities.clear();
        self.subscriptions.clear();
        self.unsubscriptions.clear();
        self.transfers.clear();
        self.changes.clear();
        self.next_operation_index = 0;
    }

    pub fn get(&self, entity_type: &str, id: &str) -> Option<EntityRecord> {
        match entity_type {
            "PoolManager" => self
                .pool_managers
                .get(id)
                .cloned()
                .map(EntityRecord::PoolManager),
            "Bundle" => self.bundles.get(id).cloned().map(EntityRecord::Bundle),
            "Token" => self.tokens.get(id).cloned().map(EntityRecord::Token),
            "Pool" => self.pools.get(id).cloned().map(EntityRecord::Pool),
            "Tick" => self.ticks.get(id).cloned().map(EntityRecord::Tick),
            "Transaction" => self
                .transactions
                .get(id)
                .cloned()
                .map(EntityRecord::Transaction),
            "Swap" => self.swaps.get(id).cloned().map(EntityRecord::Swap),
            "ModifyLiquidity" => self
                .modify_liquidities
                .get(id)
                .cloned()
                .map(EntityRecord::ModifyLiquidity),
            "UniswapDayData" => self
                .uniswap_day_data
                .get(id)
                .cloned()
                .map(EntityRecord::UniswapDayData),
            "PoolDayData" => self
                .pool_day_data
                .get(id)
                .cloned()
                .map(EntityRecord::PoolDayData),
            "PoolHourData" => self
                .pool_hour_data
                .get(id)
                .cloned()
                .map(EntityRecord::PoolHourData),
            "TokenDayData" => self
                .token_day_data
                .get(id)
                .cloned()
                .map(EntityRecord::TokenDayData),
            "TokenHourData" => self
                .token_hour_data
                .get(id)
                .cloned()
                .map(EntityRecord::TokenHourData),
            "Position" => self.positions.get(id).cloned().map(EntityRecord::Position),
            "Subscribe" => self
                .subscriptions
                .get(id)
                .cloned()
                .map(EntityRecord::Subscribe),
            "Unsubscribe" => self
                .unsubscriptions
                .get(id)
                .cloned()
                .map(EntityRecord::Unsubscribe),
            "Transfer" => self.transfers.get(id).cloned().map(EntityRecord::Transfer),
            "ArrakisHook" => self
                .arrakis_hooks
                .get(id)
                .cloned()
                .map(EntityRecord::ArrakisHook),
            _ => None,
        }
    }

    pub(crate) fn save(
        &mut self,
        entity: EntityRecord,
        event: &pb::Event,
    ) -> Result<(), StateError> {
        self.insert(entity, true, Some(event))
    }

    fn insert(
        &mut self,
        entity: EntityRecord,
        record_change: bool,
        event: Option<&pb::Event>,
    ) -> Result<(), StateError> {
        let immutable = entity.immutable();
        let entity_type = entity.entity_type();
        let id = entity.id().to_owned();
        let conflict = match &entity {
            EntityRecord::Transaction(value) => insert_immutable(&mut self.transactions, value),
            EntityRecord::Swap(value) => insert_immutable(&mut self.swaps, value),
            EntityRecord::ModifyLiquidity(value) => {
                insert_immutable(&mut self.modify_liquidities, value)
            }
            EntityRecord::Subscribe(value) => insert_immutable(&mut self.subscriptions, value),
            EntityRecord::Unsubscribe(value) => insert_immutable(&mut self.unsubscriptions, value),
            EntityRecord::Transfer(value) => insert_immutable(&mut self.transfers, value),
            EntityRecord::PoolManager(value) => {
                self.pool_managers.insert(value.id.clone(), value.clone());
                false
            }
            EntityRecord::Bundle(value) => {
                self.bundles.insert(value.id.clone(), value.clone());
                false
            }
            EntityRecord::Token(value) => {
                self.tokens.insert(value.id.clone(), value.clone());
                false
            }
            EntityRecord::Pool(value) => {
                self.pools.insert(value.id.clone(), value.clone());
                false
            }
            EntityRecord::Tick(value) => {
                self.ticks.insert(value.id.clone(), value.clone());
                false
            }
            EntityRecord::UniswapDayData(value) => {
                self.uniswap_day_data
                    .insert(value.id.clone(), value.clone());
                false
            }
            EntityRecord::PoolDayData(value) => {
                self.pool_day_data.insert(value.id.clone(), value.clone());
                false
            }
            EntityRecord::PoolHourData(value) => {
                self.pool_hour_data.insert(value.id.clone(), value.clone());
                false
            }
            EntityRecord::TokenDayData(value) => {
                self.token_day_data.insert(value.id.clone(), value.clone());
                false
            }
            EntityRecord::TokenHourData(value) => {
                self.token_hour_data.insert(value.id.clone(), value.clone());
                false
            }
            EntityRecord::Position(value) => {
                self.positions.insert(value.id.clone(), value.clone());
                false
            }
            EntityRecord::ArrakisHook(value) => {
                self.arrakis_hooks.insert(value.id.clone(), value.clone());
                false
            }
        };
        if conflict {
            return Err(StateError::ImmutableConflict {
                entity: entity_type,
                id,
            });
        }

        if record_change {
            let event = event.expect("mapping writes always have an event");
            let block = event.block.as_ref().ok_or(StateError::Missing("block"))?;
            let log = event.log.as_ref().ok_or(StateError::Missing("log"))?;
            self.changes.push(EntityChange {
                block_number: block.number,
                block_hash: block.hash.clone(),
                trigger_order: log.graph_node_trigger_order,
                operation_index: self.next_operation_index,
                immutable,
                entity,
            });
            self.next_operation_index += 1;
        }
        Ok(())
    }
}

fn insert_immutable<T>(values: &mut BTreeMap<String, T>, value: &T) -> bool
where
    T: Clone + PartialEq + Identified,
{
    match values.get(value.id()) {
        Some(existing) => existing != value,
        None => {
            values.insert(value.id().to_owned(), value.clone());
            false
        }
    }
}

trait Identified {
    fn id(&self) -> &str;
}

macro_rules! identified {
    ($($type:ty),+ $(,)?) => {$(
        impl Identified for $type {
            fn id(&self) -> &str { &self.id }
        }
    )+};
}

identified!(
    Transaction,
    Swap,
    ModifyLiquidity,
    Subscribe,
    Unsubscribe,
    Transfer
);
