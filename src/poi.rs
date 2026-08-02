//! Graph Node v0.44 fast proof-of-indexing compatibility.

use crate::{decimal::GraphDecimal, entities::*, snapshot::ReplaySnapshot, state::EntityChange};
use num_bigint::{BigInt, Sign};
use sha2::{Digest, Sha256};
use stable_hash::{
    fast::FastStableHasher, utils::AsBytes, utils::AsInt, utils::AsUnorderedSet, FieldAddress,
    StableHash, StableHasher,
};
use stable_hash_legacy::{
    crypto::{Blake3SeqNo, SetHasher},
    utils::{
        AsBytes as LegacyAsBytes, AsInt as LegacyAsInt, AsUnorderedSet as LegacyAsUnorderedSet,
    },
    SequenceNumber, StableHash as LegacyStableHash, StableHasher as LegacyStableHasher,
};
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
pub enum PoiError {
    #[error("the replay contains mapping handlers but no graft Poi$ seed")]
    MissingSeed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoiBlock {
    pub block_number: i32,
    pub id: String,
    pub digest: Vec<u8>,
}

enum PoiValue {
    String(String),
    Int(i32),
    BigDecimal(GraphDecimal),
    List(Vec<PoiValue>),
    Bytes(Vec<u8>),
    BigInt(BigInt),
}

impl StableHash for PoiValue {
    fn stable_hash<H: StableHasher>(&self, address: H::Addr, state: &mut H) {
        let variant = match self {
            Self::String(value) => {
                StableHash::stable_hash(value, address.child(0), state);
                1
            }
            Self::Int(value) => {
                StableHash::stable_hash(value, address.child(0), state);
                2
            }
            Self::BigDecimal(value) => {
                let (integer, exponent) = value.as_bigint_and_exponent();
                StableHash::stable_hash(&exponent, address.child(0).child(1), state);
                let (sign, bytes) = integer.to_bytes_le();
                AsInt {
                    is_negative: sign == graph_num_bigint::Sign::Minus,
                    little_endian: &bytes,
                }
                .stable_hash(address.child(0), state);
                3
            }
            Self::List(value) => {
                StableHash::stable_hash(value, address.child(0), state);
                5
            }
            Self::Bytes(value) => {
                StableHash::stable_hash(&AsBytes(value), address.child(0), state);
                6
            }
            Self::BigInt(value) => {
                let (sign, bytes) = value.to_bytes_le();
                AsInt {
                    is_negative: sign == Sign::Minus,
                    little_endian: &bytes,
                }
                .stable_hash(address.child(0), state);
                7
            }
        };
        state.write(address, &[variant]);
    }
}

impl LegacyStableHash for PoiValue {
    fn stable_hash<H: LegacyStableHasher>(&self, mut sequence: H::Seq, state: &mut H) {
        let variant = match self {
            Self::String(_) => "String",
            Self::Int(_) => "Int",
            Self::BigDecimal(_) => "BigDecimal",
            Self::List(_) => "List",
            Self::Bytes(_) => "Bytes",
            Self::BigInt(_) => "BigInt",
        };
        LegacyStableHash::stable_hash(&variant, sequence.next_child(), state);
        match self {
            Self::String(value) => LegacyStableHash::stable_hash(value, sequence, state),
            Self::Int(value) => LegacyStableHash::stable_hash(value, sequence, state),
            Self::BigDecimal(value) => {
                let (integer, exponent) = value.as_bigint_and_exponent();
                LegacyStableHash::stable_hash(&exponent, sequence.next_child(), state);
                let (sign, bytes) = integer.to_bytes_le();
                LegacyStableHash::stable_hash(
                    &LegacyAsInt {
                        is_negative: sign == graph_num_bigint::Sign::Minus,
                        little_endian: &bytes,
                    },
                    sequence,
                    state,
                );
            }
            Self::List(value) => LegacyStableHash::stable_hash(value, sequence, state),
            Self::Bytes(value) => {
                LegacyStableHash::stable_hash(&LegacyAsBytes(value), sequence, state)
            }
            Self::BigInt(value) => {
                let (sign, bytes) = value.to_bytes_le();
                LegacyStableHash::stable_hash(
                    &LegacyAsInt {
                        is_negative: sign == Sign::Minus,
                        little_endian: &bytes,
                    },
                    sequence,
                    state,
                );
            }
        }
    }
}

struct SetEntityEvent<'a> {
    entity_type: &'a str,
    id: &'a str,
    data: &'a [(String, PoiValue)],
}

impl StableHash for SetEntityEvent<'_> {
    fn stable_hash<H: StableHasher>(&self, address: H::Addr, state: &mut H) {
        StableHash::stable_hash(&self.entity_type, address.child(0), state);
        StableHash::stable_hash(&self.id, address.child(1), state);
        StableHash::stable_hash(&AsUnorderedSet(self.data), address.child(2), state);
        // ProofOfIndexingEvent::SetEntity is variant 2 in graph-node v0.44.
        state.write(address, &[2]);
    }
}

impl LegacyStableHash for SetEntityEvent<'_> {
    fn stable_hash<H: LegacyStableHasher>(&self, mut sequence: H::Seq, state: &mut H) {
        LegacyStableHash::stable_hash(&"SetEntity", sequence.next_child(), state);
        LegacyStableHash::stable_hash(&self.entity_type, sequence.next_child(), state);
        LegacyStableHash::stable_hash(&self.id, sequence.next_child(), state);
        LegacyStableHash::stable_hash(
            &LegacyAsUnorderedSet(self.data),
            sequence.next_child(),
            state,
        );
    }
}

fn string(value: &str) -> PoiValue {
    PoiValue::String(value.to_owned())
}

fn bigint(value: &BigInt) -> PoiValue {
    PoiValue::BigInt(value.clone())
}

fn decimal(value: &GraphDecimal) -> PoiValue {
    PoiValue::BigDecimal(value.clone())
}

fn bytes(value: &[u8]) -> PoiValue {
    PoiValue::Bytes(value.to_vec())
}

macro_rules! fields {
    ($($name:expr => $value:expr),+ $(,)?) => {
        vec![$(($name.to_owned(), $value)),+]
    };
}

fn pool_interval(value: &PoolIntervalData, start_field: &'static str) -> Vec<(String, PoiValue)> {
    let mut fields = fields![
        "id" => string(&value.id),
        start_field => PoiValue::Int(value.start),
        "pool" => string(&value.pool),
        "liquidity" => bigint(&value.liquidity),
        "sqrtPrice" => bigint(&value.sqrt_price),
        "token0Price" => decimal(&value.token0_price),
        "token1Price" => decimal(&value.token1_price),
        "tvlUSD" => decimal(&value.tvl_usd),
        "volumeToken0" => decimal(&value.volume_token0),
        "volumeToken1" => decimal(&value.volume_token1),
        "volumeUSD" => decimal(&value.volume_usd),
        "feesUSD" => decimal(&value.fees_usd),
        "txCount" => bigint(&value.tx_count),
        "open" => decimal(&value.open),
        "high" => decimal(&value.high),
        "low" => decimal(&value.low),
        "close" => decimal(&value.close),
    ];
    if let Some(tick) = &value.tick {
        fields.push(("tick".to_owned(), bigint(tick)));
    }
    fields
}

fn token_interval(value: &TokenIntervalData, start_field: &'static str) -> Vec<(String, PoiValue)> {
    fields![
        "id" => string(&value.id),
        start_field => PoiValue::Int(value.start),
        "token" => string(&value.token),
        "volume" => decimal(&value.volume),
        "volumeUSD" => decimal(&value.volume_usd),
        "untrackedVolumeUSD" => decimal(&value.untracked_volume_usd),
        "totalValueLocked" => decimal(&value.total_value_locked),
        "totalValueLockedUSD" => decimal(&value.total_value_locked_usd),
        "priceUSD" => decimal(&value.price_usd),
        "feesUSD" => decimal(&value.fees_usd),
        "open" => decimal(&value.open),
        "high" => decimal(&value.high),
        "low" => decimal(&value.low),
        "close" => decimal(&value.close),
    ]
}

fn entity_data(entity: &EntityRecord) -> Vec<(String, PoiValue)> {
    match entity {
        EntityRecord::PoolManager(value) => fields![
            "id" => string(&value.id),
            "poolCount" => bigint(&value.pool_count),
            "txCount" => bigint(&value.tx_count),
            "totalVolumeUSD" => decimal(&value.total_volume_usd),
            "totalVolumeETH" => decimal(&value.total_volume_eth),
            "totalFeesUSD" => decimal(&value.total_fees_usd),
            "totalFeesETH" => decimal(&value.total_fees_eth),
            "untrackedVolumeUSD" => decimal(&value.untracked_volume_usd),
            "totalValueLockedUSD" => decimal(&value.total_value_locked_usd),
            "totalValueLockedETH" => decimal(&value.total_value_locked_eth),
            "totalValueLockedUSDUntracked" => decimal(&value.total_value_locked_usd_untracked),
            "totalValueLockedETHUntracked" => decimal(&value.total_value_locked_eth_untracked),
            "owner" => string(&value.owner),
        ],
        EntityRecord::Bundle(value) => fields![
            "id" => string(&value.id),
            "ethPriceUSD" => decimal(&value.eth_price_usd),
        ],
        EntityRecord::Token(value) => fields![
            "id" => string(&value.id),
            "symbol" => string(&value.symbol),
            "name" => string(&value.name),
            "decimals" => bigint(&value.decimals),
            "totalSupply" => bigint(&value.total_supply),
            "volume" => decimal(&value.volume),
            "volumeUSD" => decimal(&value.volume_usd),
            "untrackedVolumeUSD" => decimal(&value.untracked_volume_usd),
            "feesUSD" => decimal(&value.fees_usd),
            "txCount" => bigint(&value.tx_count),
            "poolCount" => bigint(&value.pool_count),
            "totalValueLocked" => decimal(&value.total_value_locked),
            "totalValueLockedUSD" => decimal(&value.total_value_locked_usd),
            "totalValueLockedUSDUntracked" => decimal(&value.total_value_locked_usd_untracked),
            "derivedETH" => decimal(&value.derived_eth),
            "whitelistPools" => PoiValue::List(value.whitelist_pools.iter().map(|value| string(value)).collect()),
        ],
        EntityRecord::Pool(value) => {
            let mut fields = fields![
                "id" => string(&value.id),
                "createdAtTimestamp" => bigint(&value.created_at_timestamp),
                "createdAtBlockNumber" => bigint(&value.created_at_block_number),
                "token0" => string(&value.token0),
                "token1" => string(&value.token1),
                "feeTier" => bigint(&value.fee_tier),
                "liquidity" => bigint(&value.liquidity),
                "sqrtPrice" => bigint(&value.sqrt_price),
                "token0Price" => decimal(&value.token0_price),
                "token1Price" => decimal(&value.token1_price),
                "tickSpacing" => bigint(&value.tick_spacing),
                "observationIndex" => bigint(&value.observation_index),
                "volumeToken0" => decimal(&value.volume_token0),
                "volumeToken1" => decimal(&value.volume_token1),
                "volumeUSD" => decimal(&value.volume_usd),
                "untrackedVolumeUSD" => decimal(&value.untracked_volume_usd),
                "feesUSD" => decimal(&value.fees_usd),
                "txCount" => bigint(&value.tx_count),
                "collectedFeesToken0" => decimal(&value.collected_fees_token0),
                "collectedFeesToken1" => decimal(&value.collected_fees_token1),
                "collectedFeesUSD" => decimal(&value.collected_fees_usd),
                "totalValueLockedToken0" => decimal(&value.total_value_locked_token0),
                "totalValueLockedToken1" => decimal(&value.total_value_locked_token1),
                "totalValueLockedETH" => decimal(&value.total_value_locked_eth),
                "totalValueLockedUSD" => decimal(&value.total_value_locked_usd),
                "totalValueLockedUSDUntracked" => decimal(&value.total_value_locked_usd_untracked),
                "liquidityProviderCount" => bigint(&value.liquidity_provider_count),
                "hooks" => string(&value.hooks),
            ];
            if let Some(tick) = &value.tick {
                fields.push(("tick".to_owned(), bigint(tick)));
            }
            fields
        }
        EntityRecord::Tick(value) => {
            let mut fields = fields![
                "id" => string(&value.id),
                "tickIdx" => bigint(&value.tick_idx),
                "pool" => string(&value.pool),
                "liquidityGross" => bigint(&value.liquidity_gross),
                "liquidityNet" => bigint(&value.liquidity_net),
                "price0" => decimal(&value.price0),
                "price1" => decimal(&value.price1),
                "createdAtTimestamp" => bigint(&value.created_at_timestamp),
                "createdAtBlockNumber" => bigint(&value.created_at_block_number),
            ];
            if let Some(pool_address) = &value.pool_address {
                fields.push(("poolAddress".to_owned(), string(pool_address)));
            }
            fields
        }
        EntityRecord::Transaction(value) => fields![
            "id" => string(&value.id),
            "blockNumber" => bigint(&value.block_number),
            "timestamp" => bigint(&value.timestamp),
            "gasUsed" => bigint(&value.gas_used),
            "gasPrice" => bigint(&value.gas_price),
        ],
        EntityRecord::Swap(value) => {
            let mut fields = fields![
                "id" => string(&value.id),
                "transaction" => string(&value.transaction),
                "timestamp" => bigint(&value.timestamp),
                "pool" => string(&value.pool),
                "token0" => string(&value.token0),
                "token1" => string(&value.token1),
                "sender" => bytes(&value.sender),
                "origin" => bytes(&value.origin),
                "amount0" => decimal(&value.amount0),
                "amount1" => decimal(&value.amount1),
                "amountUSD" => decimal(&value.amount_usd),
                "sqrtPriceX96" => bigint(&value.sqrt_price_x96),
                "tick" => bigint(&value.tick),
            ];
            if let Some(log_index) = &value.log_index {
                fields.push(("logIndex".to_owned(), bigint(log_index)));
            }
            fields
        }
        EntityRecord::ModifyLiquidity(value) => {
            let mut fields = fields![
                "id" => string(&value.id),
                "transaction" => string(&value.transaction),
                "timestamp" => bigint(&value.timestamp),
                "pool" => string(&value.pool),
                "token0" => string(&value.token0),
                "token1" => string(&value.token1),
                "origin" => bytes(&value.origin),
                "amount" => bigint(&value.amount),
                "amount0" => decimal(&value.amount0),
                "amount1" => decimal(&value.amount1),
                "tickLower" => bigint(&value.tick_lower),
                "tickUpper" => bigint(&value.tick_upper),
            ];
            if let Some(sender) = &value.sender {
                fields.push(("sender".to_owned(), bytes(sender)));
            }
            if let Some(amount_usd) = &value.amount_usd {
                fields.push(("amountUSD".to_owned(), decimal(amount_usd)));
            }
            if let Some(log_index) = &value.log_index {
                fields.push(("logIndex".to_owned(), bigint(log_index)));
            }
            fields
        }
        EntityRecord::UniswapDayData(value) => fields![
            "id" => string(&value.id),
            "date" => PoiValue::Int(value.date),
            "volumeETH" => decimal(&value.volume_eth),
            "volumeUSD" => decimal(&value.volume_usd),
            "volumeUSDUntracked" => decimal(&value.volume_usd_untracked),
            "feesUSD" => decimal(&value.fees_usd),
            "txCount" => bigint(&value.tx_count),
            "tvlUSD" => decimal(&value.tvl_usd),
        ],
        EntityRecord::PoolDayData(value) => pool_interval(value, "date"),
        EntityRecord::PoolHourData(value) => pool_interval(value, "periodStartUnix"),
        EntityRecord::TokenDayData(value) => token_interval(value, "date"),
        EntityRecord::TokenHourData(value) => token_interval(value, "periodStartUnix"),
        EntityRecord::Position(value) => fields![
            "id" => string(&value.id),
            "tokenId" => bigint(&value.token_id),
            "owner" => string(&value.owner),
            "origin" => string(&value.origin),
            "createdAtTimestamp" => bigint(&value.created_at_timestamp),
        ],
        EntityRecord::Subscribe(value) => fields![
            "id" => string(&value.id),
            "tokenId" => bigint(&value.token_id),
            "address" => string(&value.address),
            "transaction" => string(&value.transaction),
            "logIndex" => bigint(&value.log_index),
            "timestamp" => bigint(&value.timestamp),
            "origin" => string(&value.origin),
            "position" => string(&value.position),
        ],
        EntityRecord::Unsubscribe(value) => fields![
            "id" => string(&value.id),
            "tokenId" => bigint(&value.token_id),
            "address" => string(&value.address),
            "transaction" => string(&value.transaction),
            "logIndex" => bigint(&value.log_index),
            "timestamp" => bigint(&value.timestamp),
            "origin" => string(&value.origin),
            "position" => string(&value.position),
        ],
        EntityRecord::Transfer(value) => fields![
            "id" => string(&value.id),
            "tokenId" => bigint(&value.token_id),
            "from" => string(&value.from),
            "to" => string(&value.to),
            "transaction" => string(&value.transaction),
            "logIndex" => bigint(&value.log_index),
            "timestamp" => bigint(&value.timestamp),
            "origin" => string(&value.origin),
            "position" => string(&value.position),
        ],
        EntityRecord::ArrakisHook(value) => fields![
            "id" => string(&value.id),
            "module" => bytes(&value.module),
            "salt" => bytes(&value.salt),
            "createdAtTimestamp" => bigint(&value.created_at_timestamp),
            "createdAtBlockNumber" => bigint(&value.created_at_block_number),
        ],
    }
}

fn event_address(block_number: u64, event_index: u64) -> u128 {
    u128::root()
        .child(1)
        .child(0)
        .child(block_number)
        .child(0)
        .child(event_index)
}

fn length_address(block_number: u64) -> u128 {
    u128::root().child(1).child(0).child(block_number).child(0)
}

fn legacy_sequence(children: &[u64]) -> Blake3SeqNo {
    children
        .iter()
        .fold(Blake3SeqNo::root(), |mut sequence, child| {
            sequence.skip(*child as usize);
            sequence.next_child()
        })
}

fn fast_block_digest(block_number: u64, changes: &[&EntityChange], previous: &[u8]) -> Vec<u8> {
    let mut hasher = FastStableHasher::new();
    for (index, change) in changes.iter().enumerate() {
        let data = entity_data(&change.entity);
        StableHash::stable_hash(
            &SetEntityEvent {
                entity_type: change.entity.entity_type(),
                id: change.entity.id(),
                data: &data,
            },
            event_address(block_number, index as u64),
            &mut hasher,
        );
    }
    StableHash::stable_hash(
        &(changes.len() as u64),
        length_address(block_number),
        &mut hasher,
    );
    let previous = if previous.len() == 32 {
        FastStableHasher::from_bytes(previous.try_into().expect("length checked"))
    } else {
        FastStableHasher::from_bytes(Sha256::digest(previous).into())
    };
    hasher.mixin(&previous);
    hasher.to_bytes().to_vec()
}

fn legacy_block_digest(block_number: u64, changes: &[&EntityChange], previous: &[u8]) -> Vec<u8> {
    let mut hasher = SetHasher::new();
    for (index, change) in changes.iter().enumerate() {
        let data = entity_data(&change.entity);
        LegacyStableHash::stable_hash(
            &SetEntityEvent {
                entity_type: change.entity.entity_type(),
                id: change.entity.id(),
                data: &data,
            },
            legacy_sequence(&[1, 0, block_number, 0, index as u64]),
            &mut hasher,
        );
    }
    LegacyStableHash::stable_hash(
        &(changes.len() as u64),
        legacy_sequence(&[1, 0, block_number, 0]),
        &mut hasher,
    );
    let previous = SetHasher::from_bytes(previous);
    hasher.finish_unordered(previous, Blake3SeqNo::root());
    hasher.to_bytes()
}

/// Return each Graph Node `Poi$` running digest produced by this replay.
pub fn digest_history(snapshot: &ReplaySnapshot) -> Result<Vec<PoiBlock>, PoiError> {
    if snapshot.state.processed_blocks.is_empty() {
        return Ok(Vec::new());
    }
    let seed = snapshot.poi_seed.as_ref().ok_or(PoiError::MissingSeed)?;
    let mut changes = BTreeMap::<u64, Vec<&EntityChange>>::new();
    for change in &snapshot.state.changes {
        changes.entry(change.block_number).or_default().push(change);
    }
    for values in changes.values_mut() {
        values.sort_by_key(|change| change.operation_index);
    }

    let mut previous = seed.digest.clone();
    let legacy = previous.len() != 32;
    let mut history = Vec::with_capacity(snapshot.state.processed_blocks.len());
    for &block_number in snapshot.state.processed_blocks.keys() {
        let block_changes = changes.get(&block_number).map(Vec::as_slice).unwrap_or(&[]);
        previous = if legacy {
            legacy_block_digest(block_number, block_changes, &previous)
        } else {
            fast_block_digest(block_number, block_changes, &previous)
        };
        history.push(PoiBlock {
            block_number: block_number as i32,
            id: seed.id.clone(),
            digest: previous.clone(),
        });
    }
    Ok(history)
}
