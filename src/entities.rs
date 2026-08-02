//! Strongly typed representations of the exact deployed GraphQL schema.

use crate::decimal::GraphDecimal;
use num_bigint::BigInt;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolManager {
    pub id: String,
    pub pool_count: BigInt,
    pub tx_count: BigInt,
    pub total_volume_usd: GraphDecimal,
    pub total_volume_eth: GraphDecimal,
    pub total_fees_usd: GraphDecimal,
    pub total_fees_eth: GraphDecimal,
    pub untracked_volume_usd: GraphDecimal,
    pub total_value_locked_usd: GraphDecimal,
    pub total_value_locked_eth: GraphDecimal,
    pub total_value_locked_usd_untracked: GraphDecimal,
    pub total_value_locked_eth_untracked: GraphDecimal,
    pub owner: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bundle {
    pub id: String,
    pub eth_price_usd: GraphDecimal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    pub id: String,
    pub symbol: String,
    pub name: String,
    pub decimals: BigInt,
    pub total_supply: BigInt,
    pub volume: GraphDecimal,
    pub volume_usd: GraphDecimal,
    pub untracked_volume_usd: GraphDecimal,
    pub fees_usd: GraphDecimal,
    pub tx_count: BigInt,
    pub pool_count: BigInt,
    pub total_value_locked: GraphDecimal,
    pub total_value_locked_usd: GraphDecimal,
    pub total_value_locked_usd_untracked: GraphDecimal,
    pub derived_eth: GraphDecimal,
    pub whitelist_pools: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pool {
    pub id: String,
    pub created_at_timestamp: BigInt,
    pub created_at_block_number: BigInt,
    pub token0: String,
    pub token1: String,
    pub fee_tier: BigInt,
    pub liquidity: BigInt,
    pub sqrt_price: BigInt,
    pub token0_price: GraphDecimal,
    pub token1_price: GraphDecimal,
    pub tick: Option<BigInt>,
    pub tick_spacing: BigInt,
    pub observation_index: BigInt,
    pub volume_token0: GraphDecimal,
    pub volume_token1: GraphDecimal,
    pub volume_usd: GraphDecimal,
    pub untracked_volume_usd: GraphDecimal,
    pub fees_usd: GraphDecimal,
    pub tx_count: BigInt,
    pub collected_fees_token0: GraphDecimal,
    pub collected_fees_token1: GraphDecimal,
    pub collected_fees_usd: GraphDecimal,
    pub total_value_locked_token0: GraphDecimal,
    pub total_value_locked_token1: GraphDecimal,
    pub total_value_locked_eth: GraphDecimal,
    pub total_value_locked_usd: GraphDecimal,
    pub total_value_locked_usd_untracked: GraphDecimal,
    pub liquidity_provider_count: BigInt,
    pub hooks: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tick {
    pub id: String,
    pub pool_address: Option<String>,
    pub tick_idx: BigInt,
    pub pool: String,
    pub liquidity_gross: BigInt,
    pub liquidity_net: BigInt,
    pub price0: GraphDecimal,
    pub price1: GraphDecimal,
    pub created_at_timestamp: BigInt,
    pub created_at_block_number: BigInt,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    pub block_number: BigInt,
    pub timestamp: BigInt,
    pub gas_used: BigInt,
    pub gas_price: BigInt,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Swap {
    pub id: String,
    pub transaction: String,
    pub timestamp: BigInt,
    pub pool: String,
    pub token0: String,
    pub token1: String,
    pub sender: Vec<u8>,
    pub origin: Vec<u8>,
    pub amount0: GraphDecimal,
    pub amount1: GraphDecimal,
    pub amount_usd: GraphDecimal,
    pub sqrt_price_x96: BigInt,
    pub tick: BigInt,
    pub log_index: Option<BigInt>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModifyLiquidity {
    pub id: String,
    pub transaction: String,
    pub timestamp: BigInt,
    pub pool: String,
    pub token0: String,
    pub token1: String,
    pub sender: Option<Vec<u8>>,
    pub origin: Vec<u8>,
    pub amount: BigInt,
    pub amount0: GraphDecimal,
    pub amount1: GraphDecimal,
    pub amount_usd: Option<GraphDecimal>,
    pub tick_lower: BigInt,
    pub tick_upper: BigInt,
    pub log_index: Option<BigInt>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UniswapDayData {
    pub id: String,
    pub date: i32,
    pub volume_eth: GraphDecimal,
    pub volume_usd: GraphDecimal,
    pub volume_usd_untracked: GraphDecimal,
    pub fees_usd: GraphDecimal,
    pub tx_count: BigInt,
    pub tvl_usd: GraphDecimal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolIntervalData {
    pub id: String,
    pub start: i32,
    pub pool: String,
    pub liquidity: BigInt,
    pub sqrt_price: BigInt,
    pub token0_price: GraphDecimal,
    pub token1_price: GraphDecimal,
    pub tick: Option<BigInt>,
    pub tvl_usd: GraphDecimal,
    pub volume_token0: GraphDecimal,
    pub volume_token1: GraphDecimal,
    pub volume_usd: GraphDecimal,
    pub fees_usd: GraphDecimal,
    pub tx_count: BigInt,
    pub open: GraphDecimal,
    pub high: GraphDecimal,
    pub low: GraphDecimal,
    pub close: GraphDecimal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenIntervalData {
    pub id: String,
    pub start: i32,
    pub token: String,
    pub volume: GraphDecimal,
    pub volume_usd: GraphDecimal,
    pub untracked_volume_usd: GraphDecimal,
    pub total_value_locked: GraphDecimal,
    pub total_value_locked_usd: GraphDecimal,
    pub price_usd: GraphDecimal,
    pub fees_usd: GraphDecimal,
    pub open: GraphDecimal,
    pub high: GraphDecimal,
    pub low: GraphDecimal,
    pub close: GraphDecimal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub id: String,
    pub token_id: BigInt,
    pub owner: String,
    pub origin: String,
    pub created_at_timestamp: BigInt,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscribe {
    pub id: String,
    pub token_id: BigInt,
    pub address: String,
    pub transaction: String,
    pub log_index: BigInt,
    pub timestamp: BigInt,
    pub origin: String,
    pub position: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unsubscribe {
    pub id: String,
    pub token_id: BigInt,
    pub address: String,
    pub transaction: String,
    pub log_index: BigInt,
    pub timestamp: BigInt,
    pub origin: String,
    pub position: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transfer {
    pub id: String,
    pub token_id: BigInt,
    pub from: String,
    pub to: String,
    pub transaction: String,
    pub log_index: BigInt,
    pub timestamp: BigInt,
    pub origin: String,
    pub position: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrakisHook {
    pub id: String,
    pub module: Vec<u8>,
    pub salt: Vec<u8>,
    pub created_at_timestamp: BigInt,
    pub created_at_block_number: BigInt,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "entity_type", content = "data")]
// Records are cloned and serialized as complete Graph entities; keeping them
// inline makes the parity model explicit and avoids per-write boxing overhead.
#[allow(clippy::large_enum_variant)]
pub enum EntityRecord {
    PoolManager(PoolManager),
    Bundle(Bundle),
    Token(Token),
    Pool(Pool),
    Tick(Tick),
    Transaction(Transaction),
    Swap(Swap),
    ModifyLiquidity(ModifyLiquidity),
    UniswapDayData(UniswapDayData),
    PoolDayData(PoolIntervalData),
    PoolHourData(PoolIntervalData),
    TokenDayData(TokenIntervalData),
    TokenHourData(TokenIntervalData),
    Position(Position),
    Subscribe(Subscribe),
    Unsubscribe(Unsubscribe),
    Transfer(Transfer),
    ArrakisHook(ArrakisHook),
}

impl EntityRecord {
    pub fn entity_type(&self) -> &'static str {
        match self {
            Self::PoolManager(_) => "PoolManager",
            Self::Bundle(_) => "Bundle",
            Self::Token(_) => "Token",
            Self::Pool(_) => "Pool",
            Self::Tick(_) => "Tick",
            Self::Transaction(_) => "Transaction",
            Self::Swap(_) => "Swap",
            Self::ModifyLiquidity(_) => "ModifyLiquidity",
            Self::UniswapDayData(_) => "UniswapDayData",
            Self::PoolDayData(_) => "PoolDayData",
            Self::PoolHourData(_) => "PoolHourData",
            Self::TokenDayData(_) => "TokenDayData",
            Self::TokenHourData(_) => "TokenHourData",
            Self::Position(_) => "Position",
            Self::Subscribe(_) => "Subscribe",
            Self::Unsubscribe(_) => "Unsubscribe",
            Self::Transfer(_) => "Transfer",
            Self::ArrakisHook(_) => "ArrakisHook",
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::PoolManager(value) => &value.id,
            Self::Bundle(value) => &value.id,
            Self::Token(value) => &value.id,
            Self::Pool(value) => &value.id,
            Self::Tick(value) => &value.id,
            Self::Transaction(value) => &value.id,
            Self::Swap(value) => &value.id,
            Self::ModifyLiquidity(value) => &value.id,
            Self::UniswapDayData(value) => &value.id,
            Self::PoolDayData(value) => &value.id,
            Self::PoolHourData(value) => &value.id,
            Self::TokenDayData(value) => &value.id,
            Self::TokenHourData(value) => &value.id,
            Self::Position(value) => &value.id,
            Self::Subscribe(value) => &value.id,
            Self::Unsubscribe(value) => &value.id,
            Self::Transfer(value) => &value.id,
            Self::ArrakisHook(value) => &value.id,
        }
    }

    pub fn immutable(&self) -> bool {
        matches!(
            self,
            Self::Transaction(_)
                | Self::Swap(_)
                | Self::ModifyLiquidity(_)
                | Self::Subscribe(_)
                | Self::Unsubscribe(_)
                | Self::Transfer(_)
        )
    }
}
