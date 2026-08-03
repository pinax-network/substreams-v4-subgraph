//! Graph Node v0.44-native Parquet materialization.

#![cfg(feature = "native")]

use crate::{
    entities::*,
    snapshot::{PoiVersion, ReplaySnapshot, SeedVersion},
};
use arrow::{
    array::{
        ArrayRef, BinaryBuilder, Int32Array, Int32Builder, Int64Array, ListBuilder, RecordBatch,
        StringBuilder,
    },
    datatypes::{DataType, Field, Schema},
};
use parquet::{
    arrow::ArrowWriter,
    basic::{Compression, ZstdLevel},
    file::properties::WriterProperties,
    file::reader::{FileReader, SerializedFileReader},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::Path,
    sync::Arc,
};

pub const DEPLOYMENT: &str = "Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB";
pub const GRAFT_BASE: &str = "QmS1ehFzXTD9eA1f1EgjZvdyAj2EHtVNMrEN91H3pLuHMy";
pub const GRAFT_BLOCK: i32 = 26_990_278;
pub const GRAFT_HASH: &str = "3365afb2ec41d64886321de82b8fd73427fda55fcf731bf5a2888c1a2a2c8948";
pub const START_BLOCK: i32 = 25_350_987;
pub const START_HASH: &str = "51b67424e303c9572ea9deebf0dace8bb453c46bf638e79a0fa545b77c79d0be";

const SCHEMA_GRAPHQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/artifacts/deployment/Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB/schema.graphql"
));
const SUBGRAPH_YAML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/artifacts/deployment/Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB/subgraph.yaml"
));

#[derive(Debug, thiserror::Error)]
pub enum SinkError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),
    #[error("Parquet error: {0}")]
    Parquet(#[from] parquet::errors::ParquetError),
    #[error("proof-of-indexing error: {0}")]
    Poi(#[from] crate::poi::PoiError),
    #[error("unknown entity type `{0}`")]
    UnknownEntity(String),
    #[error("{entity} row has {actual} fields, schema requires {expected}")]
    FieldCount {
        entity: String,
        actual: usize,
        expected: usize,
    },
    #[error("field `{field}` in {entity} has the wrong value kind")]
    CellType { entity: String, field: String },
    #[error("immutable {entity} `{id}` already exists with different data")]
    ImmutableConflict { entity: String, id: String },
    #[error("output directory already contains metadata.json: {0}")]
    ExistingDump(String),
    #[error("append target is incompatible: {0}")]
    IncompatibleAppend(String),
    #[error("existing append file failed validation: {0}")]
    InvalidAppendFile(String),
    #[error("planned stop after {0} completed tables")]
    PlannedStop(usize),
}

#[derive(Clone, Copy)]
enum CellType {
    String,
    Int32,
    Binary,
    StringList,
}

#[derive(Clone, Copy)]
struct FieldDef {
    name: &'static str,
    cell_type: CellType,
    nullable: bool,
}

impl FieldDef {
    const fn string(name: &'static str, nullable: bool) -> Self {
        Self {
            name,
            cell_type: CellType::String,
            nullable,
        }
    }

    const fn int32(name: &'static str) -> Self {
        Self {
            name,
            cell_type: CellType::Int32,
            nullable: false,
        }
    }

    const fn binary(name: &'static str, nullable: bool) -> Self {
        Self {
            name,
            cell_type: CellType::Binary,
            nullable,
        }
    }

    const fn string_list(name: &'static str) -> Self {
        Self {
            name,
            cell_type: CellType::StringList,
            nullable: false,
        }
    }
}

struct TableDef {
    entity_type: &'static str,
    immutable: bool,
    fields: &'static [FieldDef],
}

const POOL_MANAGER_FIELDS: &[FieldDef] = &[
    FieldDef::string("id", false),
    FieldDef::string("pool_count", false),
    FieldDef::string("tx_count", false),
    FieldDef::string("total_volume_usd", false),
    FieldDef::string("total_volume_eth", false),
    FieldDef::string("total_fees_usd", false),
    FieldDef::string("total_fees_eth", false),
    FieldDef::string("untracked_volume_usd", false),
    FieldDef::string("total_value_locked_usd", false),
    FieldDef::string("total_value_locked_eth", false),
    FieldDef::string("total_value_locked_usd_untracked", false),
    FieldDef::string("total_value_locked_eth_untracked", false),
    FieldDef::string("owner", false),
];
const BUNDLE_FIELDS: &[FieldDef] = &[
    FieldDef::string("id", false),
    FieldDef::string("eth_price_usd", false),
];
const TOKEN_FIELDS: &[FieldDef] = &[
    FieldDef::string("id", false),
    FieldDef::string("symbol", false),
    FieldDef::string("name", false),
    FieldDef::string("decimals", false),
    FieldDef::string("total_supply", false),
    FieldDef::string("volume", false),
    FieldDef::string("volume_usd", false),
    FieldDef::string("untracked_volume_usd", false),
    FieldDef::string("fees_usd", false),
    FieldDef::string("tx_count", false),
    FieldDef::string("pool_count", false),
    FieldDef::string("total_value_locked", false),
    FieldDef::string("total_value_locked_usd", false),
    FieldDef::string("total_value_locked_usd_untracked", false),
    FieldDef::string("derived_eth", false),
    FieldDef::string_list("whitelist_pools"),
];
const POOL_FIELDS: &[FieldDef] = &[
    FieldDef::string("id", false),
    FieldDef::string("created_at_timestamp", false),
    FieldDef::string("created_at_block_number", false),
    FieldDef::string("token_0", false),
    FieldDef::string("token_1", false),
    FieldDef::string("fee_tier", false),
    FieldDef::string("liquidity", false),
    FieldDef::string("sqrt_price", false),
    FieldDef::string("token_0_price", false),
    FieldDef::string("token_1_price", false),
    FieldDef::string("tick", true),
    FieldDef::string("tick_spacing", false),
    FieldDef::string("observation_index", false),
    FieldDef::string("volume_token_0", false),
    FieldDef::string("volume_token_1", false),
    FieldDef::string("volume_usd", false),
    FieldDef::string("untracked_volume_usd", false),
    FieldDef::string("fees_usd", false),
    FieldDef::string("tx_count", false),
    FieldDef::string("collected_fees_token_0", false),
    FieldDef::string("collected_fees_token_1", false),
    FieldDef::string("collected_fees_usd", false),
    FieldDef::string("total_value_locked_token_0", false),
    FieldDef::string("total_value_locked_token_1", false),
    FieldDef::string("total_value_locked_eth", false),
    FieldDef::string("total_value_locked_usd", false),
    FieldDef::string("total_value_locked_usd_untracked", false),
    FieldDef::string("liquidity_provider_count", false),
    FieldDef::string("hooks", false),
];
const TICK_FIELDS: &[FieldDef] = &[
    FieldDef::string("id", false),
    FieldDef::string("pool_address", true),
    FieldDef::string("tick_idx", false),
    FieldDef::string("pool", false),
    FieldDef::string("liquidity_gross", false),
    FieldDef::string("liquidity_net", false),
    FieldDef::string("price_0", false),
    FieldDef::string("price_1", false),
    FieldDef::string("created_at_timestamp", false),
    FieldDef::string("created_at_block_number", false),
];
const TRANSACTION_FIELDS: &[FieldDef] = &[
    FieldDef::string("id", false),
    FieldDef::string("block_number", false),
    FieldDef::string("timestamp", false),
    FieldDef::string("gas_used", false),
    FieldDef::string("gas_price", false),
];
const SWAP_FIELDS: &[FieldDef] = &[
    FieldDef::string("id", false),
    FieldDef::string("transaction", false),
    FieldDef::string("timestamp", false),
    FieldDef::string("pool", false),
    FieldDef::string("token_0", false),
    FieldDef::string("token_1", false),
    FieldDef::binary("sender", false),
    FieldDef::binary("origin", false),
    FieldDef::string("amount_0", false),
    FieldDef::string("amount_1", false),
    FieldDef::string("amount_usd", false),
    FieldDef::string("sqrt_price_x96", false),
    FieldDef::string("tick", false),
    FieldDef::string("log_index", true),
];
const MODIFY_FIELDS: &[FieldDef] = &[
    FieldDef::string("id", false),
    FieldDef::string("transaction", false),
    FieldDef::string("timestamp", false),
    FieldDef::string("pool", false),
    FieldDef::string("token_0", false),
    FieldDef::string("token_1", false),
    FieldDef::binary("sender", true),
    FieldDef::binary("origin", false),
    FieldDef::string("amount", false),
    FieldDef::string("amount_0", false),
    FieldDef::string("amount_1", false),
    FieldDef::string("amount_usd", true),
    FieldDef::string("tick_lower", false),
    FieldDef::string("tick_upper", false),
    FieldDef::string("log_index", true),
];
const UNISWAP_DAY_FIELDS: &[FieldDef] = &[
    FieldDef::string("id", false),
    FieldDef::int32("date"),
    FieldDef::string("volume_eth", false),
    FieldDef::string("volume_usd", false),
    FieldDef::string("volume_usd_untracked", false),
    FieldDef::string("fees_usd", false),
    FieldDef::string("tx_count", false),
    FieldDef::string("tvl_usd", false),
];
const POOL_INTERVAL_FIELDS: &[FieldDef] = &[
    FieldDef::string("id", false),
    FieldDef::int32("date"),
    FieldDef::string("pool", false),
    FieldDef::string("liquidity", false),
    FieldDef::string("sqrt_price", false),
    FieldDef::string("token_0_price", false),
    FieldDef::string("token_1_price", false),
    FieldDef::string("tick", true),
    FieldDef::string("tvl_usd", false),
    FieldDef::string("volume_token_0", false),
    FieldDef::string("volume_token_1", false),
    FieldDef::string("volume_usd", false),
    FieldDef::string("fees_usd", false),
    FieldDef::string("tx_count", false),
    FieldDef::string("open", false),
    FieldDef::string("high", false),
    FieldDef::string("low", false),
    FieldDef::string("close", false),
];
const POOL_HOUR_FIELDS: &[FieldDef] = &[
    FieldDef::string("id", false),
    FieldDef::int32("period_start_unix"),
    FieldDef::string("pool", false),
    FieldDef::string("liquidity", false),
    FieldDef::string("sqrt_price", false),
    FieldDef::string("token_0_price", false),
    FieldDef::string("token_1_price", false),
    FieldDef::string("tick", true),
    FieldDef::string("tvl_usd", false),
    FieldDef::string("volume_token_0", false),
    FieldDef::string("volume_token_1", false),
    FieldDef::string("volume_usd", false),
    FieldDef::string("fees_usd", false),
    FieldDef::string("tx_count", false),
    FieldDef::string("open", false),
    FieldDef::string("high", false),
    FieldDef::string("low", false),
    FieldDef::string("close", false),
];
const TOKEN_INTERVAL_FIELDS: &[FieldDef] = &[
    FieldDef::string("id", false),
    FieldDef::int32("date"),
    FieldDef::string("token", false),
    FieldDef::string("volume", false),
    FieldDef::string("volume_usd", false),
    FieldDef::string("untracked_volume_usd", false),
    FieldDef::string("total_value_locked", false),
    FieldDef::string("total_value_locked_usd", false),
    FieldDef::string("price_usd", false),
    FieldDef::string("fees_usd", false),
    FieldDef::string("open", false),
    FieldDef::string("high", false),
    FieldDef::string("low", false),
    FieldDef::string("close", false),
];
const TOKEN_HOUR_FIELDS: &[FieldDef] = &[
    FieldDef::string("id", false),
    FieldDef::int32("period_start_unix"),
    FieldDef::string("token", false),
    FieldDef::string("volume", false),
    FieldDef::string("volume_usd", false),
    FieldDef::string("untracked_volume_usd", false),
    FieldDef::string("total_value_locked", false),
    FieldDef::string("total_value_locked_usd", false),
    FieldDef::string("price_usd", false),
    FieldDef::string("fees_usd", false),
    FieldDef::string("open", false),
    FieldDef::string("high", false),
    FieldDef::string("low", false),
    FieldDef::string("close", false),
];
const POSITION_FIELDS: &[FieldDef] = &[
    FieldDef::string("id", false),
    FieldDef::string("token_id", false),
    FieldDef::string("owner", false),
    FieldDef::string("origin", false),
    FieldDef::string("created_at_timestamp", false),
];
const SUBSCRIBE_FIELDS: &[FieldDef] = &[
    FieldDef::string("id", false),
    FieldDef::string("token_id", false),
    FieldDef::string("address", false),
    FieldDef::string("transaction", false),
    FieldDef::string("log_index", false),
    FieldDef::string("timestamp", false),
    FieldDef::string("origin", false),
    FieldDef::string("position", false),
];
const TRANSFER_FIELDS: &[FieldDef] = &[
    FieldDef::string("id", false),
    FieldDef::string("token_id", false),
    FieldDef::string("from", false),
    FieldDef::string("to", false),
    FieldDef::string("transaction", false),
    FieldDef::string("log_index", false),
    FieldDef::string("timestamp", false),
    FieldDef::string("origin", false),
    FieldDef::string("position", false),
];
const ARRAKIS_FIELDS: &[FieldDef] = &[
    FieldDef::string("id", false),
    FieldDef::binary("module", false),
    FieldDef::binary("salt", false),
    FieldDef::string("created_at_timestamp", false),
    FieldDef::string("created_at_block_number", false),
];
const POI_FIELDS: &[FieldDef] = &[
    FieldDef::binary("digest", false),
    FieldDef::string("id", false),
];

const TABLES: &[TableDef] = &[
    TableDef {
        entity_type: "ArrakisHook",
        immutable: false,
        fields: ARRAKIS_FIELDS,
    },
    TableDef {
        entity_type: "Bundle",
        immutable: false,
        fields: BUNDLE_FIELDS,
    },
    TableDef {
        entity_type: "ModifyLiquidity",
        immutable: true,
        fields: MODIFY_FIELDS,
    },
    TableDef {
        entity_type: "Poi$",
        immutable: false,
        fields: POI_FIELDS,
    },
    TableDef {
        entity_type: "Pool",
        immutable: false,
        fields: POOL_FIELDS,
    },
    TableDef {
        entity_type: "PoolDayData",
        immutable: false,
        fields: POOL_INTERVAL_FIELDS,
    },
    TableDef {
        entity_type: "PoolHourData",
        immutable: false,
        fields: POOL_HOUR_FIELDS,
    },
    TableDef {
        entity_type: "PoolManager",
        immutable: false,
        fields: POOL_MANAGER_FIELDS,
    },
    TableDef {
        entity_type: "Position",
        immutable: false,
        fields: POSITION_FIELDS,
    },
    TableDef {
        entity_type: "Subscribe",
        immutable: true,
        fields: SUBSCRIBE_FIELDS,
    },
    TableDef {
        entity_type: "Swap",
        immutable: true,
        fields: SWAP_FIELDS,
    },
    TableDef {
        entity_type: "Tick",
        immutable: false,
        fields: TICK_FIELDS,
    },
    TableDef {
        entity_type: "Token",
        immutable: false,
        fields: TOKEN_FIELDS,
    },
    TableDef {
        entity_type: "TokenDayData",
        immutable: false,
        fields: TOKEN_INTERVAL_FIELDS,
    },
    TableDef {
        entity_type: "TokenHourData",
        immutable: false,
        fields: TOKEN_HOUR_FIELDS,
    },
    TableDef {
        entity_type: "Transaction",
        immutable: true,
        fields: TRANSACTION_FIELDS,
    },
    TableDef {
        entity_type: "Transfer",
        immutable: true,
        fields: TRANSFER_FIELDS,
    },
    TableDef {
        entity_type: "UniswapDayData",
        immutable: false,
        fields: UNISWAP_DAY_FIELDS,
    },
    TableDef {
        entity_type: "Unsubscribe",
        immutable: true,
        fields: SUBSCRIBE_FIELDS,
    },
];

fn table(entity_type: &str) -> Result<&'static TableDef, SinkError> {
    TABLES
        .iter()
        .find(|table| table.entity_type == entity_type)
        .ok_or_else(|| SinkError::UnknownEntity(entity_type.to_owned()))
}

#[derive(Clone)]
enum Cell {
    String(String),
    Int32(i32),
    Binary(Vec<u8>),
    StringList(Vec<String>),
    Null,
}

fn string<T: ToString>(value: &T) -> Cell {
    Cell::String(value.to_string())
}

fn optional_string<T: ToString>(value: &Option<T>) -> Cell {
    value.as_ref().map(string).unwrap_or(Cell::Null)
}

fn pool_interval_cells(value: &PoolIntervalData) -> Vec<Cell> {
    vec![
        string(&value.id),
        Cell::Int32(value.start),
        string(&value.pool),
        string(&value.liquidity),
        string(&value.sqrt_price),
        string(&value.token0_price),
        string(&value.token1_price),
        optional_string(&value.tick),
        string(&value.tvl_usd),
        string(&value.volume_token0),
        string(&value.volume_token1),
        string(&value.volume_usd),
        string(&value.fees_usd),
        string(&value.tx_count),
        string(&value.open),
        string(&value.high),
        string(&value.low),
        string(&value.close),
    ]
}

fn token_interval_cells(value: &TokenIntervalData) -> Vec<Cell> {
    vec![
        string(&value.id),
        Cell::Int32(value.start),
        string(&value.token),
        string(&value.volume),
        string(&value.volume_usd),
        string(&value.untracked_volume_usd),
        string(&value.total_value_locked),
        string(&value.total_value_locked_usd),
        string(&value.price_usd),
        string(&value.fees_usd),
        string(&value.open),
        string(&value.high),
        string(&value.low),
        string(&value.close),
    ]
}

fn entity_cells(entity: &EntityRecord) -> Vec<Cell> {
    match entity {
        EntityRecord::PoolManager(v) => vec![
            string(&v.id),
            string(&v.pool_count),
            string(&v.tx_count),
            string(&v.total_volume_usd),
            string(&v.total_volume_eth),
            string(&v.total_fees_usd),
            string(&v.total_fees_eth),
            string(&v.untracked_volume_usd),
            string(&v.total_value_locked_usd),
            string(&v.total_value_locked_eth),
            string(&v.total_value_locked_usd_untracked),
            string(&v.total_value_locked_eth_untracked),
            string(&v.owner),
        ],
        EntityRecord::Bundle(v) => vec![string(&v.id), string(&v.eth_price_usd)],
        EntityRecord::Token(v) => vec![
            string(&v.id),
            string(&v.symbol),
            string(&v.name),
            string(&v.decimals),
            string(&v.total_supply),
            string(&v.volume),
            string(&v.volume_usd),
            string(&v.untracked_volume_usd),
            string(&v.fees_usd),
            string(&v.tx_count),
            string(&v.pool_count),
            string(&v.total_value_locked),
            string(&v.total_value_locked_usd),
            string(&v.total_value_locked_usd_untracked),
            string(&v.derived_eth),
            Cell::StringList(v.whitelist_pools.clone()),
        ],
        EntityRecord::Pool(v) => vec![
            string(&v.id),
            string(&v.created_at_timestamp),
            string(&v.created_at_block_number),
            string(&v.token0),
            string(&v.token1),
            string(&v.fee_tier),
            string(&v.liquidity),
            string(&v.sqrt_price),
            string(&v.token0_price),
            string(&v.token1_price),
            optional_string(&v.tick),
            string(&v.tick_spacing),
            string(&v.observation_index),
            string(&v.volume_token0),
            string(&v.volume_token1),
            string(&v.volume_usd),
            string(&v.untracked_volume_usd),
            string(&v.fees_usd),
            string(&v.tx_count),
            string(&v.collected_fees_token0),
            string(&v.collected_fees_token1),
            string(&v.collected_fees_usd),
            string(&v.total_value_locked_token0),
            string(&v.total_value_locked_token1),
            string(&v.total_value_locked_eth),
            string(&v.total_value_locked_usd),
            string(&v.total_value_locked_usd_untracked),
            string(&v.liquidity_provider_count),
            string(&v.hooks),
        ],
        EntityRecord::Tick(v) => vec![
            string(&v.id),
            optional_string(&v.pool_address),
            string(&v.tick_idx),
            string(&v.pool),
            string(&v.liquidity_gross),
            string(&v.liquidity_net),
            string(&v.price0),
            string(&v.price1),
            string(&v.created_at_timestamp),
            string(&v.created_at_block_number),
        ],
        EntityRecord::Transaction(v) => vec![
            string(&v.id),
            string(&v.block_number),
            string(&v.timestamp),
            string(&v.gas_used),
            string(&v.gas_price),
        ],
        EntityRecord::Swap(v) => vec![
            string(&v.id),
            string(&v.transaction),
            string(&v.timestamp),
            string(&v.pool),
            string(&v.token0),
            string(&v.token1),
            Cell::Binary(v.sender.clone()),
            Cell::Binary(v.origin.clone()),
            string(&v.amount0),
            string(&v.amount1),
            string(&v.amount_usd),
            string(&v.sqrt_price_x96),
            string(&v.tick),
            optional_string(&v.log_index),
        ],
        EntityRecord::ModifyLiquidity(v) => vec![
            string(&v.id),
            string(&v.transaction),
            string(&v.timestamp),
            string(&v.pool),
            string(&v.token0),
            string(&v.token1),
            v.sender.clone().map(Cell::Binary).unwrap_or(Cell::Null),
            Cell::Binary(v.origin.clone()),
            string(&v.amount),
            string(&v.amount0),
            string(&v.amount1),
            optional_string(&v.amount_usd),
            string(&v.tick_lower),
            string(&v.tick_upper),
            optional_string(&v.log_index),
        ],
        EntityRecord::UniswapDayData(v) => vec![
            string(&v.id),
            Cell::Int32(v.date),
            string(&v.volume_eth),
            string(&v.volume_usd),
            string(&v.volume_usd_untracked),
            string(&v.fees_usd),
            string(&v.tx_count),
            string(&v.tvl_usd),
        ],
        EntityRecord::PoolDayData(v) | EntityRecord::PoolHourData(v) => pool_interval_cells(v),
        EntityRecord::TokenDayData(v) | EntityRecord::TokenHourData(v) => token_interval_cells(v),
        EntityRecord::Position(v) => vec![
            string(&v.id),
            string(&v.token_id),
            string(&v.owner),
            string(&v.origin),
            string(&v.created_at_timestamp),
        ],
        EntityRecord::Subscribe(v) => vec![
            string(&v.id),
            string(&v.token_id),
            string(&v.address),
            string(&v.transaction),
            string(&v.log_index),
            string(&v.timestamp),
            string(&v.origin),
            string(&v.position),
        ],
        EntityRecord::Unsubscribe(v) => vec![
            string(&v.id),
            string(&v.token_id),
            string(&v.address),
            string(&v.transaction),
            string(&v.log_index),
            string(&v.timestamp),
            string(&v.origin),
            string(&v.position),
        ],
        EntityRecord::Transfer(v) => vec![
            string(&v.id),
            string(&v.token_id),
            string(&v.from),
            string(&v.to),
            string(&v.transaction),
            string(&v.log_index),
            string(&v.timestamp),
            string(&v.origin),
            string(&v.position),
        ],
        EntityRecord::ArrakisHook(v) => vec![
            string(&v.id),
            Cell::Binary(v.module.clone()),
            Cell::Binary(v.salt.clone()),
            string(&v.created_at_timestamp),
            string(&v.created_at_block_number),
        ],
    }
}

#[derive(Clone)]
enum RowData {
    Entity(Box<EntityRecord>),
    Poi { id: String, digest: Vec<u8> },
}

impl RowData {
    fn entity_type(&self) -> &'static str {
        match self {
            Self::Entity(entity) => entity.entity_type(),
            Self::Poi { .. } => "Poi$",
        }
    }

    fn id(&self) -> &str {
        match self {
            Self::Entity(entity) => entity.id(),
            Self::Poi { id, .. } => id,
        }
    }

    fn cells(&self) -> Vec<Cell> {
        match self {
            Self::Entity(entity) => entity_cells(entity),
            Self::Poi { id, digest } => vec![Cell::Binary(digest.clone()), string(id)],
        }
    }
}

#[derive(Clone)]
struct VersionRow {
    vid: i64,
    block_start: i32,
    block_end: Option<i32>,
    data: RowData,
}

#[derive(Clone, Copy)]
enum Location {
    Seed,
    New(usize),
}

#[derive(Clone)]
struct ActiveVersion {
    vid: i64,
    block_start: i32,
    data: RowData,
    location: Location,
}

#[derive(Default)]
struct TableRows {
    seed: Vec<VersionRow>,
    new: Vec<VersionRow>,
    clamps: BTreeMap<i64, i32>,
    max_vid: i64,
}

pub struct DumpBuilder {
    tables: BTreeMap<String, TableRows>,
    active: BTreeMap<(String, String), ActiveVersion>,
    initial_entity_keys: BTreeSet<(String, String)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AppendJournal {
    version: u32,
    deployment: String,
    snapshot_sha256: String,
    seed_head: BlockPtr,
    target_head: BlockPtr,
    tables: BTreeMap<String, TableAppend>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TableAppend {
    chunk: Option<ChunkInfo>,
    chunk_sha256: Option<String>,
    clamp: Option<ChunkInfo>,
    clamp_sha256: Option<String>,
    max_vid: i64,
}

impl DumpBuilder {
    pub fn from_snapshot(snapshot: &ReplaySnapshot) -> Result<Self, SinkError> {
        let mut this = Self {
            tables: TABLES
                .iter()
                .map(|def| {
                    let max_vid = snapshot
                        .table_max_vids
                        .get(def.entity_type)
                        .copied()
                        .unwrap_or(-1);
                    (
                        def.entity_type.to_owned(),
                        TableRows {
                            max_vid,
                            ..Default::default()
                        },
                    )
                })
                .collect(),
            active: BTreeMap::new(),
            initial_entity_keys: BTreeSet::new(),
        };
        for seed in &snapshot.seed_versions {
            this.insert_seed(seed)?;
            this.initial_entity_keys.insert((
                seed.entity.entity_type().to_owned(),
                seed.entity.id().to_owned(),
            ));
        }
        if let Some(seed) = &snapshot.poi_seed {
            this.insert_poi_seed(seed)?;
        }
        this.apply_entity_changes(snapshot)?;
        this.apply_poi_changes(snapshot)?;
        Ok(this)
    }

    fn insert_seed(&mut self, seed: &SeedVersion) -> Result<(), SinkError> {
        let data = RowData::Entity(Box::new(seed.entity.clone()));
        self.insert_seed_row(seed.vid, seed.block_range_start, data)
    }

    fn insert_poi_seed(&mut self, seed: &PoiVersion) -> Result<(), SinkError> {
        self.insert_seed_row(
            seed.vid,
            seed.block_range_start,
            RowData::Poi {
                id: seed.id.clone(),
                digest: seed.digest.clone(),
            },
        )
    }

    fn insert_seed_row(
        &mut self,
        vid: i64,
        block_start: i32,
        data: RowData,
    ) -> Result<(), SinkError> {
        let entity_type = data.entity_type();
        table(entity_type)?;
        let key = (entity_type.to_owned(), data.id().to_owned());
        let rows = self
            .tables
            .get_mut(entity_type)
            .expect("all tables initialized");
        rows.max_vid = rows.max_vid.max(vid);
        rows.seed.push(VersionRow {
            vid,
            block_start,
            block_end: None,
            data: data.clone(),
        });
        self.active.insert(
            key,
            ActiveVersion {
                vid,
                block_start,
                data,
                location: Location::Seed,
            },
        );
        Ok(())
    }

    fn apply_entity_changes(&mut self, snapshot: &ReplaySnapshot) -> Result<(), SinkError> {
        let mut final_per_block = BTreeMap::<(u64, String, String), EntityRecord>::new();
        for change in &snapshot.state.changes {
            final_per_block.insert(
                (
                    change.block_number,
                    change.entity.entity_type().to_owned(),
                    change.entity.id().to_owned(),
                ),
                change.entity.clone(),
            );
        }
        for ((block, _, _), entity) in final_per_block {
            self.apply_entity(block as i32, entity)?;
        }
        Ok(())
    }

    fn apply_entity(&mut self, block: i32, entity: EntityRecord) -> Result<(), SinkError> {
        let entity_type = entity.entity_type();
        let def = table(entity_type)?;
        let id = entity.id().to_owned();
        let key = (entity_type.to_owned(), id.clone());
        let data = RowData::Entity(Box::new(entity));
        if let Some(current) = self.active.get(&key).cloned() {
            if matches!((&current.data, &data), (RowData::Entity(left), RowData::Entity(right)) if left == right)
            {
                return Ok(());
            }
            if def.immutable {
                return Err(SinkError::ImmutableConflict {
                    entity: entity_type.to_owned(),
                    id,
                });
            }
            let rows = self
                .tables
                .get_mut(entity_type)
                .expect("all tables initialized");
            match current.location {
                Location::Seed => {
                    rows.clamps.insert(current.vid, block);
                }
                Location::New(index) => {
                    rows.new[index].block_end = Some(block);
                }
            }
        }
        let rows = self
            .tables
            .get_mut(entity_type)
            .expect("all tables initialized");
        rows.max_vid += 1;
        let vid = rows.max_vid;
        let index = rows.new.len();
        rows.new.push(VersionRow {
            vid,
            block_start: block,
            block_end: None,
            data: data.clone(),
        });
        self.active.insert(
            key,
            ActiveVersion {
                vid,
                block_start: block,
                data,
                location: Location::New(index),
            },
        );
        Ok(())
    }

    fn apply_poi_changes(&mut self, snapshot: &ReplaySnapshot) -> Result<(), SinkError> {
        for poi in crate::poi::digest_history(snapshot)? {
            let entity_type = "Poi$";
            let key = (entity_type.to_owned(), poi.id.clone());
            if let Some(current) = self.active.get(&key).cloned() {
                let rows = self
                    .tables
                    .get_mut(entity_type)
                    .expect("all tables initialized");
                match current.location {
                    Location::Seed => {
                        rows.clamps.insert(current.vid, poi.block_number);
                    }
                    Location::New(index) => {
                        rows.new[index].block_end = Some(poi.block_number);
                    }
                }
            }
            let rows = self
                .tables
                .get_mut(entity_type)
                .expect("all tables initialized");
            rows.max_vid += 1;
            let vid = rows.max_vid;
            let data = RowData::Poi {
                id: poi.id.clone(),
                digest: poi.digest,
            };
            let index = rows.new.len();
            rows.new.push(VersionRow {
                vid,
                block_start: poi.block_number,
                block_end: None,
                data: data.clone(),
            });
            self.active.insert(
                key,
                ActiveVersion {
                    vid,
                    block_start: poi.block_number,
                    data,
                    location: Location::New(index),
                },
            );
        }
        Ok(())
    }

    pub fn resume_checkpoint(&self, snapshot: &ReplaySnapshot) -> ReplaySnapshot {
        let mut state = snapshot.state.clone();
        state.prepare_resume_checkpoint();
        let seed_versions = self
            .active
            .values()
            .filter_map(|version| match &version.data {
                RowData::Entity(entity) if !entity.immutable() => Some(SeedVersion {
                    vid: version.vid,
                    block_range_start: version.block_start,
                    entity: entity.as_ref().clone(),
                }),
                _ => None,
            })
            .collect();
        let poi_seed = self
            .active
            .values()
            .find_map(|version| match &version.data {
                RowData::Poi { id, digest } => Some(PoiVersion {
                    vid: version.vid,
                    block_range_start: version.block_start,
                    id: id.clone(),
                    digest: digest.clone(),
                }),
                _ => None,
            });
        ReplaySnapshot {
            state,
            seed_versions,
            table_max_vids: self
                .tables
                .iter()
                .map(|(entity_type, rows)| (entity_type.clone(), rows.max_vid))
                .collect(),
            poi_seed,
        }
    }

    fn new_entity_count(&self) -> usize {
        self.active
            .keys()
            .filter(|(entity_type, _)| entity_type != "Poi$")
            .filter(|key| !self.initial_entity_keys.contains(*key))
            .count()
    }

    pub fn append(
        mut self,
        output: &Path,
        snapshot: &ReplaySnapshot,
        head_block: i32,
        head_hash: &str,
        stop_after_tables: Option<usize>,
    ) -> Result<Metadata, SinkError> {
        let metadata_path = output.join("metadata.json");
        let mut metadata: Metadata = serde_json::from_slice(&fs::read(&metadata_path)?)?;
        validate_append_identity(output, &metadata)?;
        let target_head = BlockPtr {
            number: head_block,
            hash: normalize_hash(head_hash),
        };
        let seed_head = metadata.head_block.clone().ok_or_else(|| {
            SinkError::IncompatibleAppend("seed metadata has no head block".to_owned())
        })?;
        if seed_head.number == target_head.number && seed_head.hash == target_head.hash {
            let journal_path = output.join(".substreams-append.json");
            if journal_path.exists() {
                fs::remove_file(journal_path)?;
            }
            return Ok(metadata);
        }
        if seed_head.number >= target_head.number {
            return Err(SinkError::IncompatibleAppend(format!(
                "seed head {} must be before target head {}",
                seed_head.number, target_head.number
            )));
        }
        for def in TABLES {
            let table_info = metadata.tables.get(def.entity_type).ok_or_else(|| {
                SinkError::IncompatibleAppend(format!(
                    "seed metadata is missing table {}",
                    def.entity_type
                ))
            })?;
            let snapshot_max = snapshot
                .table_max_vids
                .get(def.entity_type)
                .copied()
                .unwrap_or(-1);
            if table_info.max_vid != snapshot_max {
                return Err(SinkError::IncompatibleAppend(format!(
                    "{} seed max VID is {}, snapshot expects {}",
                    def.entity_type, table_info.max_vid, snapshot_max
                )));
            }
        }

        let snapshot_sha256 = format!("{:x}", Sha256::digest(serde_json::to_vec(snapshot)?));
        let journal_path = output.join(".substreams-append.json");
        let mut journal = if journal_path.exists() {
            let value: AppendJournal = serde_json::from_slice(&fs::read(&journal_path)?)?;
            if value.version != 1
                || value.deployment != DEPLOYMENT
                || value.snapshot_sha256 != snapshot_sha256
                || value.seed_head.number != seed_head.number
                || value.seed_head.hash != seed_head.hash
                || value.target_head.number != target_head.number
                || value.target_head.hash != target_head.hash
            {
                return Err(SinkError::IncompatibleAppend(
                    "resume journal does not describe this exact append".to_owned(),
                ));
            }
            value
        } else {
            let value = AppendJournal {
                version: 1,
                deployment: DEPLOYMENT.to_owned(),
                snapshot_sha256,
                seed_head: seed_head.clone(),
                target_head: target_head.clone(),
                tables: BTreeMap::new(),
            };
            write_atomic(&journal_path, &serde_json::to_vec_pretty(&value)?)?;
            value
        };

        let new_entity_count = self.new_entity_count();
        for def in TABLES {
            let seed_table = metadata
                .tables
                .get(def.entity_type)
                .expect("validated table metadata");
            if let Some(done) = journal.tables.get(def.entity_type) {
                validate_optional_append_file(output, &done.chunk, &done.chunk_sha256)?;
                validate_optional_append_file(output, &done.clamp, &done.clamp_sha256)?;
                continue;
            }
            let mut rows = self
                .tables
                .remove(def.entity_type)
                .expect("all tables initialized");
            rows.new.sort_by_key(|row| row.vid);
            let chunk = if rows.new.is_empty() {
                None
            } else {
                let index = seed_table.chunks.len();
                let expected = chunk_info(def.entity_type, "chunk", index, &rows.new);
                Some(write_or_validate_chunk(
                    output, def, index, &rows.new, expected,
                )?)
            };
            let clamp = if rows.clamps.is_empty() {
                None
            } else {
                let index = seed_table.clamps.len();
                let expected = clamp_info(def.entity_type, index, &rows.clamps);
                Some(write_or_validate_clamp(
                    output,
                    def.entity_type,
                    index,
                    &rows.clamps,
                    expected,
                )?)
            };
            journal.tables.insert(
                def.entity_type.to_owned(),
                TableAppend {
                    chunk_sha256: optional_file_sha256(output, &chunk)?,
                    clamp_sha256: optional_file_sha256(output, &clamp)?,
                    chunk,
                    clamp,
                    max_vid: rows.max_vid,
                },
            );
            write_atomic(&journal_path, &serde_json::to_vec_pretty(&journal)?)?;
            if stop_after_tables.is_some_and(|limit| journal.tables.len() >= limit) {
                return Err(SinkError::PlannedStop(journal.tables.len()));
            }
        }

        for def in TABLES {
            let appended = journal
                .tables
                .get(def.entity_type)
                .expect("every table completed before metadata commit");
            let table_info = metadata
                .tables
                .get_mut(def.entity_type)
                .expect("validated table metadata");
            if let Some(chunk) = &appended.chunk {
                table_info.chunks.push(chunk.clone());
            }
            if let Some(clamp) = &appended.clamp {
                table_info.clamps.push(clamp.clone());
            }
            table_info.max_vid = appended.max_vid;
        }
        metadata.head_block = Some(target_head);
        metadata.entity_count = metadata
            .entity_count
            .checked_add(new_entity_count)
            .ok_or_else(|| SinkError::IncompatibleAppend("entity count overflow".to_owned()))?;
        write_atomic(&metadata_path, &serde_json::to_vec_pretty(&metadata)?)?;
        fs::remove_file(journal_path)?;
        Ok(metadata)
    }

    pub fn write(
        mut self,
        output: &Path,
        head_block: i32,
        head_hash: &str,
    ) -> Result<Metadata, SinkError> {
        if output.join("metadata.json").exists() {
            return Err(SinkError::ExistingDump(output.display().to_string()));
        }
        fs::create_dir_all(output)?;
        write_atomic(output.join("schema.graphql"), SCHEMA_GRAPHQL.as_bytes())?;
        write_atomic(output.join("subgraph.yaml"), SUBGRAPH_YAML.as_bytes())?;

        let mut table_info = BTreeMap::new();
        for def in TABLES {
            let mut rows = self
                .tables
                .remove(def.entity_type)
                .expect("all tables initialized");
            rows.seed.sort_by_key(|row| row.vid);
            rows.new.sort_by_key(|row| row.vid);
            let mut chunks = Vec::new();
            if !rows.seed.is_empty() {
                chunks.push(write_chunk(output, def, 0, &rows.seed)?);
            }
            if !rows.new.is_empty() {
                chunks.push(write_chunk(output, def, chunks.len(), &rows.new)?);
            }
            let clamps = if rows.clamps.is_empty() {
                Vec::new()
            } else {
                vec![write_clamp(output, def.entity_type, 0, &rows.clamps)?]
            };
            table_info.insert(
                def.entity_type.to_owned(),
                TableInfo {
                    immutable: def.immutable,
                    has_causality_region: false,
                    chunks,
                    clamps,
                    max_vid: rows.max_vid,
                },
            );
        }
        table_info.insert(
            "data_sources$".to_owned(),
            TableInfo {
                immutable: false,
                has_causality_region: true,
                chunks: Vec::new(),
                clamps: Vec::new(),
                max_vid: -1,
            },
        );

        let metadata = Metadata {
            version: 1,
            deployment: DEPLOYMENT.to_owned(),
            network: "base".to_owned(),
            manifest: Manifest {
                spec_version: "0.0.4".to_owned(),
                description: Some(
                    "Uniswap is a decentralized protocol for automated token exchange on Ethereum."
                        .to_owned(),
                ),
                repository: Some("https://github.com/Uniswap/v4-subgraph".to_owned()),
                features: vec!["nonFatalErrors".to_owned(), "grafting".to_owned()],
                entities_with_causality_region: Vec::new(),
                history_blocks: i32::MAX,
            },
            earliest_block_number: START_BLOCK,
            start_block: Some(BlockPtr {
                number: START_BLOCK,
                hash: START_HASH.to_owned(),
            }),
            head_block: Some(BlockPtr {
                number: head_block,
                hash: normalize_hash(head_hash),
            }),
            entity_count: self
                .active
                .keys()
                .filter(|(entity_type, _)| entity_type != "Poi$")
                .count(),
            graft_base: Some(GRAFT_BASE.to_owned()),
            graft_block: Some(BlockPtr {
                number: GRAFT_BLOCK,
                hash: GRAFT_HASH.to_owned(),
            }),
            debug_fork: None,
            health: Health {
                failed: false,
                health: "healthy".to_owned(),
                fatal_error: None,
                non_fatal_errors: Vec::new(),
            },
            indexes: BTreeMap::new(),
            tables: table_info,
        };
        let bytes = serde_json::to_vec_pretty(&metadata)?;
        write_atomic(output.join("metadata.json"), &bytes)?;
        Ok(metadata)
    }
}

fn normalize_hash(value: &str) -> String {
    value
        .strip_prefix("0x")
        .unwrap_or(value)
        .to_ascii_lowercase()
}

fn write_atomic(path: impl AsRef<Path>, bytes: &[u8]) -> Result<(), std::io::Error> {
    let path = path.as_ref();
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, path)
}

fn arrow_schema(def: &TableDef) -> Schema {
    let mut fields = vec![Field::new("vid", DataType::Int64, false)];
    if def.immutable {
        fields.push(Field::new("block$", DataType::Int32, false));
    } else {
        fields.push(Field::new("block_range_start", DataType::Int32, false));
        fields.push(Field::new("block_range_end", DataType::Int32, true));
    }
    fields.extend(def.fields.iter().map(|field| {
        let data_type = match field.cell_type {
            CellType::String => DataType::Utf8,
            CellType::Int32 => DataType::Int32,
            CellType::Binary => DataType::Binary,
            CellType::StringList => {
                DataType::List(Arc::new(Field::new("item", DataType::Utf8, true)))
            }
        };
        Field::new(field.name, data_type, field.nullable)
    }));
    Schema::new(fields)
}

fn record_batch(def: &TableDef, rows: &[VersionRow]) -> Result<RecordBatch, SinkError> {
    let schema = arrow_schema(def);
    let mut arrays: Vec<ArrayRef> = vec![Arc::new(Int64Array::from(
        rows.iter().map(|row| row.vid).collect::<Vec<_>>(),
    ))];
    arrays.push(Arc::new(Int32Array::from(
        rows.iter().map(|row| row.block_start).collect::<Vec<_>>(),
    )));
    if !def.immutable {
        arrays.push(Arc::new(Int32Array::from(
            rows.iter().map(|row| row.block_end).collect::<Vec<_>>(),
        )));
    }
    let cells = rows.iter().map(|row| row.data.cells()).collect::<Vec<_>>();
    for row in &cells {
        if row.len() != def.fields.len() {
            return Err(SinkError::FieldCount {
                entity: def.entity_type.to_owned(),
                actual: row.len(),
                expected: def.fields.len(),
            });
        }
    }
    for (index, field) in def.fields.iter().enumerate() {
        arrays.push(build_column(def.entity_type, field, &cells, index)?);
    }
    RecordBatch::try_new(Arc::new(schema), arrays).map_err(SinkError::from)
}

fn build_column(
    entity_type: &str,
    field: &FieldDef,
    rows: &[Vec<Cell>],
    index: usize,
) -> Result<ArrayRef, SinkError> {
    let wrong = || SinkError::CellType {
        entity: entity_type.to_owned(),
        field: field.name.to_owned(),
    };
    Ok(match field.cell_type {
        CellType::String => {
            let mut builder = StringBuilder::new();
            for row in rows {
                match &row[index] {
                    Cell::String(value) => builder.append_value(value),
                    Cell::Null if field.nullable => builder.append_null(),
                    _ => return Err(wrong()),
                }
            }
            Arc::new(builder.finish())
        }
        CellType::Int32 => {
            let mut builder = Int32Builder::new();
            for row in rows {
                match row[index] {
                    Cell::Int32(value) => builder.append_value(value),
                    Cell::Null if field.nullable => builder.append_null(),
                    _ => return Err(wrong()),
                }
            }
            Arc::new(builder.finish())
        }
        CellType::Binary => {
            let mut builder = BinaryBuilder::new();
            for row in rows {
                match &row[index] {
                    Cell::Binary(value) => builder.append_value(value),
                    Cell::Null if field.nullable => builder.append_null(),
                    _ => return Err(wrong()),
                }
            }
            Arc::new(builder.finish())
        }
        CellType::StringList => {
            let mut builder = ListBuilder::new(StringBuilder::new());
            for row in rows {
                match &row[index] {
                    Cell::StringList(values) => {
                        for value in values {
                            builder.values().append_value(value);
                        }
                        builder.append(true);
                    }
                    Cell::Null if field.nullable => builder.append(false),
                    _ => return Err(wrong()),
                }
            }
            Arc::new(builder.finish())
        }
    })
}

fn writer_properties() -> WriterProperties {
    WriterProperties::builder()
        .set_compression(Compression::ZSTD(ZstdLevel::default()))
        .build()
}

fn validate_append_identity(output: &Path, metadata: &Metadata) -> Result<(), SinkError> {
    if metadata.version != 1 || metadata.deployment != DEPLOYMENT || metadata.network != "base" {
        return Err(SinkError::IncompatibleAppend(
            "metadata is not the pinned Base deployment".to_owned(),
        ));
    }
    if fs::read_to_string(output.join("schema.graphql"))? != SCHEMA_GRAPHQL
        || fs::read_to_string(output.join("subgraph.yaml"))? != SUBGRAPH_YAML
    {
        return Err(SinkError::IncompatibleAppend(
            "schema or manifest differs from the pinned deployment".to_owned(),
        ));
    }
    let expected = table_names();
    let actual = metadata
        .tables
        .keys()
        .filter(|name| name.as_str() != "data_sources$")
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if actual != expected || !metadata.tables.contains_key("data_sources$") {
        return Err(SinkError::IncompatibleAppend(
            "table inventory differs from the pinned deployment".to_owned(),
        ));
    }
    Ok(())
}

fn chunk_info(entity_type: &str, prefix: &str, index: usize, rows: &[VersionRow]) -> ChunkInfo {
    ChunkInfo {
        file: format!("{entity_type}/{prefix}_{index:06}.parquet"),
        min_vid: rows.first().expect("nonempty chunk").vid,
        max_vid: rows.last().expect("nonempty chunk").vid,
        row_count: rows.len(),
    }
}

fn clamp_info(entity_type: &str, index: usize, clamps: &BTreeMap<i64, i32>) -> ChunkInfo {
    ChunkInfo {
        file: format!("{entity_type}/clamp_{index:06}.parquet"),
        min_vid: *clamps.first_key_value().expect("nonempty clamps").0,
        max_vid: *clamps.last_key_value().expect("nonempty clamps").0,
        row_count: clamps.len(),
    }
}

fn validate_optional_append_file(
    output: &Path,
    info: &Option<ChunkInfo>,
    expected_sha256: &Option<String>,
) -> Result<(), SinkError> {
    if let Some(info) = info {
        let path = output.join(&info.file);
        validate_parquet_rows(&path, info.row_count)?;
        let actual = file_sha256(&path)?;
        if expected_sha256.as_ref() != Some(&actual) {
            return Err(SinkError::InvalidAppendFile(format!(
                "{} SHA-256 differs from resume journal",
                path.display()
            )));
        }
    } else if expected_sha256.is_some() {
        return Err(SinkError::InvalidAppendFile(
            "resume journal has a hash without a file".to_owned(),
        ));
    }
    Ok(())
}

fn optional_file_sha256(
    output: &Path,
    info: &Option<ChunkInfo>,
) -> Result<Option<String>, SinkError> {
    info.as_ref()
        .map(|info| file_sha256(&output.join(&info.file)))
        .transpose()
}

fn file_sha256(path: &Path) -> Result<String, SinkError> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn validate_parquet_rows(path: &Path, expected: usize) -> Result<(), SinkError> {
    let file = fs::File::open(path)
        .map_err(|_| SinkError::InvalidAppendFile(path.display().to_string()))?;
    let reader = SerializedFileReader::new(file)?;
    let actual = reader.metadata().file_metadata().num_rows() as usize;
    if actual != expected {
        return Err(SinkError::InvalidAppendFile(format!(
            "{} has {actual} rows, expected {expected}",
            path.display()
        )));
    }
    Ok(())
}

fn write_or_validate_chunk(
    output: &Path,
    def: &TableDef,
    index: usize,
    rows: &[VersionRow],
    expected: ChunkInfo,
) -> Result<ChunkInfo, SinkError> {
    let path = output.join(&expected.file);
    if path.exists() {
        validate_parquet_rows(&path, expected.row_count)?;
        Ok(expected)
    } else {
        write_chunk(output, def, index, rows)
    }
}

fn write_or_validate_clamp(
    output: &Path,
    entity_type: &str,
    index: usize,
    clamps: &BTreeMap<i64, i32>,
    expected: ChunkInfo,
) -> Result<ChunkInfo, SinkError> {
    let path = output.join(&expected.file);
    if path.exists() {
        validate_parquet_rows(&path, expected.row_count)?;
        Ok(expected)
    } else {
        write_clamp(output, entity_type, index, clamps)
    }
}

fn write_chunk(
    output: &Path,
    def: &TableDef,
    index: usize,
    rows: &[VersionRow],
) -> Result<ChunkInfo, SinkError> {
    let table_dir = output.join(def.entity_type);
    fs::create_dir_all(&table_dir)?;
    let info = chunk_info(def.entity_type, "chunk", index, rows);
    let final_path = output.join(&info.file);
    let temporary_path = final_path.with_extension("parquet.tmp");
    if temporary_path.exists() {
        fs::remove_file(&temporary_path)?;
    }
    let file = fs::File::create(&temporary_path)?;
    let schema = arrow_schema(def);
    let mut writer = ArrowWriter::try_new(file, Arc::new(schema), Some(writer_properties()))?;
    writer.write(&record_batch(def, rows)?)?;
    writer.close()?;
    fs::rename(temporary_path, final_path)?;
    Ok(info)
}

fn write_clamp(
    output: &Path,
    entity_type: &str,
    index: usize,
    clamps: &BTreeMap<i64, i32>,
) -> Result<ChunkInfo, SinkError> {
    let table_dir = output.join(entity_type);
    fs::create_dir_all(&table_dir)?;
    let info = clamp_info(entity_type, index, clamps);
    let final_path = output.join(&info.file);
    let temporary_path = final_path.with_extension("parquet.tmp");
    if temporary_path.exists() {
        fs::remove_file(&temporary_path)?;
    }
    let file = fs::File::create(&temporary_path)?;
    let schema = Schema::new(vec![
        Field::new("vid", DataType::Int64, false),
        Field::new("block_range_end", DataType::Int32, false),
    ]);
    let batch = RecordBatch::try_new(
        Arc::new(schema.clone()),
        vec![
            Arc::new(Int64Array::from(clamps.keys().copied().collect::<Vec<_>>())),
            Arc::new(Int32Array::from(
                clamps.values().copied().collect::<Vec<_>>(),
            )),
        ],
    )?;
    let mut writer = ArrowWriter::try_new(file, Arc::new(schema), Some(writer_properties()))?;
    writer.write(&batch)?;
    writer.close()?;
    fs::rename(temporary_path, final_path)?;
    Ok(info)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChunkInfo {
    pub file: String,
    pub min_vid: i64,
    pub max_vid: i64,
    pub row_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TableInfo {
    pub immutable: bool,
    pub has_causality_region: bool,
    pub chunks: Vec<ChunkInfo>,
    pub clamps: Vec<ChunkInfo>,
    pub max_vid: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub spec_version: String,
    pub description: Option<String>,
    pub repository: Option<String>,
    pub features: Vec<String>,
    pub entities_with_causality_region: Vec<String>,
    pub history_blocks: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockPtr {
    pub number: i32,
    pub hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Health {
    pub failed: bool,
    pub health: String,
    pub fatal_error: Option<serde_json::Value>,
    pub non_fatal_errors: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Metadata {
    pub version: u32,
    pub deployment: String,
    pub network: String,
    pub manifest: Manifest,
    pub earliest_block_number: i32,
    pub start_block: Option<BlockPtr>,
    pub head_block: Option<BlockPtr>,
    pub entity_count: usize,
    pub graft_base: Option<String>,
    pub graft_block: Option<BlockPtr>,
    pub debug_fork: Option<String>,
    pub health: Health,
    pub indexes: BTreeMap<String, Vec<String>>,
    pub tables: BTreeMap<String, TableInfo>,
}

pub fn validate_head_hash(value: &str) -> bool {
    let value = value.strip_prefix("0x").unwrap_or(value);
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn table_names() -> BTreeSet<&'static str> {
    TABLES.iter().map(|table| table.entity_type).collect()
}
