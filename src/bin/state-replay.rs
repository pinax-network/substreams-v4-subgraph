use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::io::{self, BufRead};
use std::path::PathBuf;
use substreams_v4_subgraph::{
    decimal::GraphDecimal,
    entities::*,
    pb::pinax::uniswap::v4::base::v1 as pb,
    snapshot::{PoiVersion, ReplaySnapshot, SeedVersion},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = args()?;
    let initial: ReplaySnapshot = match &args.input_snapshot {
        Some(path) => serde_json::from_slice(&fs::read(path)?)?,
        None => ReplaySnapshot::default(),
    };
    let mut initial = initial;
    if let Some(path) = &args.supplement_seeds {
        let supplemental: ReplaySnapshot = serde_json::from_slice(&fs::read(path)?)?;
        initial.supplement_missing_seeds(&supplemental)?;
    }
    let mut state = initial.state;
    let mut expected = Vec::new();
    let mut seed_versions = initial.seed_versions;
    let mut table_max_vids = initial.table_max_vids;
    let mut poi_seed = initial.poi_seed;
    let mut poi_expected = None;
    for line in io::stdin().lock().lines() {
        let line = line?;
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(value) = value.get("@seed") {
            state.insert_seed(record(value)?)?;
            continue;
        }
        if let Some(value) = value.get("@seed_version") {
            let entity = record(value)?;
            let data = object(value, "data")?;
            seed_versions.push(SeedVersion {
                vid: i64_value(data, "vid")?,
                block_range_start: i64_value(data, "block_range_start")? as i32,
                entity: entity.clone(),
            });
            state.insert_seed(entity)?;
            continue;
        }
        if let Some(value) = value.get("@table") {
            table_max_vids.insert(
                string_value(value, "entity_type")?,
                i64_value(value, "max_vid")?,
            );
            continue;
        }
        if let Some(value) = value.get("@poi_seed") {
            poi_seed = Some(PoiVersion {
                vid: i64_value(value, "vid")?,
                block_range_start: i64_value(value, "block_range_start")? as i32,
                id: string_value(value, "id")?,
                digest: decode_bytea(&string_value(value, "digest")?, "digest")?,
            });
            continue;
        }
        if let Some(value) = value.get("@poi_expected") {
            poi_expected = Some((
                string_value(value, "id")?,
                decode_bytea(&string_value(value, "digest")?, "digest")?,
            ));
            continue;
        }
        if let Some(value) = value.get("@expected") {
            expected.push(record(value)?);
            continue;
        }
        let Some(events) = value
            .get("@data")
            .and_then(|value| value.get("events"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        state.apply(&pb::Events {
            events: events.iter().map(event).collect::<Result<_, _>>()?,
        })?;
    }
    if let Some(path) = args.output_snapshot {
        write_snapshot(
            path,
            &ReplaySnapshot {
                state: state.clone(),
                seed_versions: seed_versions.clone(),
                table_max_vids: table_max_vids.clone(),
                poi_seed: poi_seed.clone(),
            },
        )?;
    }
    if expected.is_empty() && !args.quiet {
        serde_json::to_writer(io::stdout().lock(), &state)?;
    } else if !expected.is_empty() {
        let mut mismatches = Vec::new();
        let poi_checked = poi_expected.is_some();
        let expected_keys = expected
            .iter()
            .map(|record| (record.entity_type(), record.id().to_owned()))
            .collect::<BTreeSet<_>>();
        let changed_keys = state
            .changes
            .iter()
            .map(|change| (change.entity.entity_type(), change.entity.id().to_owned()))
            .collect::<BTreeSet<_>>();
        for expected in &expected {
            let actual = state.get(expected.entity_type(), expected.id());
            if actual.as_ref() != Some(expected) {
                mismatches.push(serde_json::json!({
                    "kind": "value_mismatch",
                    "entity_type": expected.entity_type(),
                    "id": expected.id(),
                    "expected": expected,
                    "actual": actual,
                }));
            }
        }
        for (entity_type, id) in &changed_keys {
            if !expected_keys.contains(&(entity_type, id.clone())) {
                mismatches.push(serde_json::json!({
                    "kind": "unexpected_actual",
                    "entity_type": entity_type,
                    "id": id,
                    "expected": null,
                    "actual": state.get(entity_type, id),
                }));
            }
        }
        if let Some((expected_id, expected_digest)) = poi_expected {
            let poi_snapshot = ReplaySnapshot {
                state: state.clone(),
                seed_versions: seed_versions.clone(),
                table_max_vids: table_max_vids.clone(),
                poi_seed: poi_seed.clone(),
            };
            let actual = substreams_v4_subgraph::poi::digest_history(&poi_snapshot)?
                .last()
                .cloned();
            if actual.as_ref().map(|poi| (&poi.id, &poi.digest))
                != Some((&expected_id, &expected_digest))
            {
                mismatches.push(serde_json::json!({
                    "kind": "poi_mismatch",
                    "expected": {
                        "id": expected_id,
                        "digest": hex(&expected_digest),
                    },
                    "actual": actual.map(|poi| serde_json::json!({
                        "id": poi.id,
                        "digest": hex(&poi.digest),
                    })),
                }));
            }
        }
        serde_json::to_writer(
            io::stdout().lock(),
            &serde_json::json!({
                "expected_entities": expected.len(),
                "changed_entities": changed_keys.len(),
                "poi_checked": poi_checked,
                "mismatch_count": mismatches.len(),
                "mismatches": mismatches,
            }),
        )?;
        if !mismatches.is_empty() {
            return Err(format!("{} entity mismatches", mismatches.len()).into());
        }
    }
    Ok(())
}

fn hex(value: &[u8]) -> String {
    let mut output = String::with_capacity(value.len() * 2);
    for byte in value {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to a String is infallible");
    }
    output
}

struct Args {
    input_snapshot: Option<PathBuf>,
    output_snapshot: Option<PathBuf>,
    supplement_seeds: Option<PathBuf>,
    quiet: bool,
}

fn args() -> Result<Args, String> {
    let mut args = std::env::args_os().skip(1);
    let mut input_snapshot = None;
    let mut output_snapshot = None;
    let mut supplement_seeds = None;
    let mut quiet = false;
    while let Some(arg) = args.next() {
        if arg == "--quiet" {
            quiet = true;
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| format!("{} requires a path", arg.to_string_lossy()))?;
        match arg.to_string_lossy().as_ref() {
            "--snapshot" => output_snapshot = Some(PathBuf::from(value)),
            "--input-snapshot" => input_snapshot = Some(PathBuf::from(value)),
            "--supplement-seeds" => supplement_seeds = Some(PathBuf::from(value)),
            value => return Err(format!("unknown argument `{value}`")),
        }
    }
    Ok(Args {
        input_snapshot,
        output_snapshot,
        supplement_seeds,
        quiet,
    })
}

fn write_snapshot(path: PathBuf, snapshot: &ReplaySnapshot) -> Result<(), io::Error> {
    let tmp = path.with_extension("tmp");
    let bytes = serde_json::to_vec(snapshot).expect("snapshot serialization is infallible");
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, path)
}

fn record(value: &Value) -> Result<EntityRecord, String> {
    let entity_type = string_value(value, "entity_type")?;
    let data = object(value, "data")?;
    let id = string_value(data, "id")?;
    Ok(match entity_type.as_str() {
        "PoolManager" => EntityRecord::PoolManager(PoolManager {
            id,
            pool_count: bigint(data, "pool_count")?,
            tx_count: bigint(data, "tx_count")?,
            total_volume_usd: decimal(data, "total_volume_usd")?,
            total_volume_eth: decimal(data, "total_volume_eth")?,
            total_fees_usd: decimal(data, "total_fees_usd")?,
            total_fees_eth: decimal(data, "total_fees_eth")?,
            untracked_volume_usd: decimal(data, "untracked_volume_usd")?,
            total_value_locked_usd: decimal(data, "total_value_locked_usd")?,
            total_value_locked_eth: decimal(data, "total_value_locked_eth")?,
            total_value_locked_usd_untracked: decimal(data, "total_value_locked_usd_untracked")?,
            total_value_locked_eth_untracked: decimal(data, "total_value_locked_eth_untracked")?,
            owner: string_value(data, "owner")?,
        }),
        "Bundle" => EntityRecord::Bundle(Bundle {
            id,
            eth_price_usd: decimal(data, "eth_price_usd")?,
        }),
        "Token" => EntityRecord::Token(Token {
            id,
            symbol: string_value(data, "symbol")?,
            name: string_value(data, "name")?,
            decimals: bigint(data, "decimals")?,
            total_supply: bigint(data, "total_supply")?,
            volume: decimal(data, "volume")?,
            volume_usd: decimal(data, "volume_usd")?,
            untracked_volume_usd: decimal(data, "untracked_volume_usd")?,
            fees_usd: decimal(data, "fees_usd")?,
            tx_count: bigint(data, "tx_count")?,
            pool_count: bigint(data, "pool_count")?,
            total_value_locked: decimal(data, "total_value_locked")?,
            total_value_locked_usd: decimal(data, "total_value_locked_usd")?,
            total_value_locked_usd_untracked: decimal(data, "total_value_locked_usd_untracked")?,
            derived_eth: decimal(data, "derived_eth")?,
            whitelist_pools: strings(data, "whitelist_pools")?,
        }),
        "Pool" => EntityRecord::Pool(Pool {
            id,
            created_at_timestamp: bigint(data, "created_at_timestamp")?,
            created_at_block_number: bigint(data, "created_at_block_number")?,
            token0: string_value(data, "token_0")?,
            token1: string_value(data, "token_1")?,
            fee_tier: bigint(data, "fee_tier")?,
            liquidity: bigint(data, "liquidity")?,
            sqrt_price: bigint(data, "sqrt_price")?,
            token0_price: decimal(data, "token_0_price")?,
            token1_price: decimal(data, "token_1_price")?,
            tick: optional_bigint(data, "tick")?,
            tick_spacing: bigint(data, "tick_spacing")?,
            observation_index: bigint(data, "observation_index")?,
            volume_token0: decimal(data, "volume_token_0")?,
            volume_token1: decimal(data, "volume_token_1")?,
            volume_usd: decimal(data, "volume_usd")?,
            untracked_volume_usd: decimal(data, "untracked_volume_usd")?,
            fees_usd: decimal(data, "fees_usd")?,
            tx_count: bigint(data, "tx_count")?,
            collected_fees_token0: decimal(data, "collected_fees_token_0")?,
            collected_fees_token1: decimal(data, "collected_fees_token_1")?,
            collected_fees_usd: decimal(data, "collected_fees_usd")?,
            total_value_locked_token0: decimal(data, "total_value_locked_token_0")?,
            total_value_locked_token1: decimal(data, "total_value_locked_token_1")?,
            total_value_locked_eth: decimal(data, "total_value_locked_eth")?,
            total_value_locked_usd: decimal(data, "total_value_locked_usd")?,
            total_value_locked_usd_untracked: decimal(data, "total_value_locked_usd_untracked")?,
            liquidity_provider_count: bigint(data, "liquidity_provider_count")?,
            hooks: string_value(data, "hooks")?,
        }),
        "Tick" => EntityRecord::Tick(Tick {
            id,
            pool_address: optional_string(data, "pool_address")?,
            tick_idx: bigint(data, "tick_idx")?,
            pool: string_value(data, "pool")?,
            liquidity_gross: bigint(data, "liquidity_gross")?,
            liquidity_net: bigint(data, "liquidity_net")?,
            price0: decimal(data, "price_0")?,
            price1: decimal(data, "price_1")?,
            created_at_timestamp: bigint(data, "created_at_timestamp")?,
            created_at_block_number: bigint(data, "created_at_block_number")?,
        }),
        "Transaction" => EntityRecord::Transaction(Transaction {
            id,
            block_number: bigint(data, "block_number")?,
            timestamp: bigint(data, "timestamp")?,
            gas_used: bigint(data, "gas_used")?,
            gas_price: bigint(data, "gas_price")?,
        }),
        "Swap" => EntityRecord::Swap(Swap {
            id,
            transaction: string_value(data, "transaction")?,
            timestamp: bigint(data, "timestamp")?,
            pool: string_value(data, "pool")?,
            token0: string_value(data, "token_0")?,
            token1: string_value(data, "token_1")?,
            sender: bytea(data, "sender")?,
            origin: bytea(data, "origin")?,
            amount0: decimal(data, "amount_0")?,
            amount1: decimal(data, "amount_1")?,
            amount_usd: decimal(data, "amount_usd")?,
            sqrt_price_x96: bigint(data, "sqrt_price_x96")?,
            tick: bigint(data, "tick")?,
            log_index: optional_bigint(data, "log_index")?,
        }),
        "ModifyLiquidity" => EntityRecord::ModifyLiquidity(ModifyLiquidity {
            id,
            transaction: string_value(data, "transaction")?,
            timestamp: bigint(data, "timestamp")?,
            pool: string_value(data, "pool")?,
            token0: string_value(data, "token_0")?,
            token1: string_value(data, "token_1")?,
            sender: optional_bytea(data, "sender")?,
            origin: bytea(data, "origin")?,
            amount: bigint(data, "amount")?,
            amount0: decimal(data, "amount_0")?,
            amount1: decimal(data, "amount_1")?,
            amount_usd: optional_decimal(data, "amount_usd")?,
            tick_lower: bigint(data, "tick_lower")?,
            tick_upper: bigint(data, "tick_upper")?,
            log_index: optional_bigint(data, "log_index")?,
        }),
        "UniswapDayData" => EntityRecord::UniswapDayData(UniswapDayData {
            id,
            date: i32_field(data, "date")?,
            volume_eth: decimal(data, "volume_eth")?,
            volume_usd: decimal(data, "volume_usd")?,
            volume_usd_untracked: decimal(data, "volume_usd_untracked")?,
            fees_usd: decimal(data, "fees_usd")?,
            tx_count: bigint(data, "tx_count")?,
            tvl_usd: decimal(data, "tvl_usd")?,
        }),
        "PoolDayData" => EntityRecord::PoolDayData(pool_interval(data, id, "date")?),
        "PoolHourData" => EntityRecord::PoolHourData(pool_interval(data, id, "period_start_unix")?),
        "TokenDayData" => EntityRecord::TokenDayData(token_interval(data, id, "date")?),
        "TokenHourData" => {
            EntityRecord::TokenHourData(token_interval(data, id, "period_start_unix")?)
        }
        "Position" => EntityRecord::Position(Position {
            id,
            token_id: bigint(data, "token_id")?,
            owner: string_value(data, "owner")?,
            origin: string_value(data, "origin")?,
            created_at_timestamp: bigint(data, "created_at_timestamp")?,
        }),
        "Subscribe" => EntityRecord::Subscribe(Subscribe {
            id,
            token_id: bigint(data, "token_id")?,
            address: string_value(data, "address")?,
            transaction: string_value(data, "transaction")?,
            log_index: bigint(data, "log_index")?,
            timestamp: bigint(data, "timestamp")?,
            origin: string_value(data, "origin")?,
            position: string_value(data, "position")?,
        }),
        "Unsubscribe" => EntityRecord::Unsubscribe(Unsubscribe {
            id,
            token_id: bigint(data, "token_id")?,
            address: string_value(data, "address")?,
            transaction: string_value(data, "transaction")?,
            log_index: bigint(data, "log_index")?,
            timestamp: bigint(data, "timestamp")?,
            origin: string_value(data, "origin")?,
            position: string_value(data, "position")?,
        }),
        "Transfer" => EntityRecord::Transfer(Transfer {
            id,
            token_id: bigint(data, "token_id")?,
            from: string_value(data, "from")?,
            to: string_value(data, "to")?,
            transaction: string_value(data, "transaction")?,
            log_index: bigint(data, "log_index")?,
            timestamp: bigint(data, "timestamp")?,
            origin: string_value(data, "origin")?,
            position: string_value(data, "position")?,
        }),
        "ArrakisHook" => EntityRecord::ArrakisHook(ArrakisHook {
            id,
            module: bytea(data, "module")?,
            salt: bytea(data, "salt")?,
            created_at_timestamp: bigint(data, "created_at_timestamp")?,
            created_at_block_number: bigint(data, "created_at_block_number")?,
        }),
        _ => return Err(format!("unknown entity type `{entity_type}`")),
    })
}

fn pool_interval(data: &Value, id: String, start_field: &str) -> Result<PoolIntervalData, String> {
    Ok(PoolIntervalData {
        id,
        start: i32_field(data, start_field)?,
        pool: string_value(data, "pool")?,
        liquidity: bigint(data, "liquidity")?,
        sqrt_price: bigint(data, "sqrt_price")?,
        token0_price: decimal(data, "token_0_price")?,
        token1_price: decimal(data, "token_1_price")?,
        tick: optional_bigint(data, "tick")?,
        tvl_usd: decimal(data, "tvl_usd")?,
        volume_token0: decimal(data, "volume_token_0")?,
        volume_token1: decimal(data, "volume_token_1")?,
        volume_usd: decimal(data, "volume_usd")?,
        fees_usd: decimal(data, "fees_usd")?,
        tx_count: bigint(data, "tx_count")?,
        open: decimal(data, "open")?,
        high: decimal(data, "high")?,
        low: decimal(data, "low")?,
        close: decimal(data, "close")?,
    })
}

fn token_interval(
    data: &Value,
    id: String,
    start_field: &str,
) -> Result<TokenIntervalData, String> {
    Ok(TokenIntervalData {
        id,
        start: i32_field(data, start_field)?,
        token: string_value(data, "token")?,
        volume: decimal(data, "volume")?,
        volume_usd: decimal(data, "volume_usd")?,
        untracked_volume_usd: decimal(data, "untracked_volume_usd")?,
        total_value_locked: decimal(data, "total_value_locked")?,
        total_value_locked_usd: decimal(data, "total_value_locked_usd")?,
        price_usd: decimal(data, "price_usd")?,
        fees_usd: decimal(data, "fees_usd")?,
        open: decimal(data, "open")?,
        high: decimal(data, "high")?,
        low: decimal(data, "low")?,
        close: decimal(data, "close")?,
    })
}

fn event(value: &Value) -> Result<pb::Event, String> {
    let block = object(value, "block")?;
    let transaction = object(value, "transaction")?;
    let log = object(value, "log")?;
    Ok(pb::Event {
        block: Some(pb::BlockRef {
            number: u64_value(block, "number")?,
            hash: bytes(block, "hash")?,
            parent_hash: bytes(block, "parentHash")?,
            timestamp_seconds: i64_value(block, "timestampSeconds")?,
            timestamp_nanos: optional_i64(block, "timestampNanos")? as i32,
        }),
        transaction: Some(pb::TransactionRef {
            index: optional_u64(transaction, "index")? as u32,
            hash: bytes(transaction, "hash")?,
            origin: bytes(transaction, "origin")?,
            to: bytes(transaction, "to")?,
            gas_price: string_value(transaction, "gasPrice")?,
        }),
        log: Some(pb::LogRef {
            transaction_log_index: optional_u64(log, "transactionLogIndex")? as u32,
            block_log_index: optional_u64(log, "blockLogIndex")? as u32,
            ordinal: optional_u64(log, "ordinal")?,
            address: bytes(log, "address")?,
            graph_node_trigger_order: optional_u64(log, "graphNodeTriggerOrder")?,
        }),
        source: pb::DataSource::Unspecified as i32,
        payload: Some(payload(value)?),
    })
}

fn payload(value: &Value) -> Result<pb::event::Payload, String> {
    if let Some(value) = value.get("initialize") {
        return Ok(pb::event::Payload::Initialize(pb::Initialize {
            pool_id: bytes(value, "poolId")?,
            currency0: bytes(value, "currency0")?,
            currency1: bytes(value, "currency1")?,
            fee: string_value(value, "fee")?,
            tick_spacing: string_value(value, "tickSpacing")?,
            hooks: bytes(value, "hooks")?,
            sqrt_price_x96: string_value(value, "sqrtPriceX96")?,
            tick: string_value(value, "tick")?,
            token0_metadata: Some(metadata(object(value, "token0Metadata")?)?),
            token1_metadata: Some(metadata(object(value, "token1Metadata")?)?),
        }));
    }
    if let Some(value) = value.get("modifyLiquidity") {
        return Ok(pb::event::Payload::ModifyLiquidity(pb::ModifyLiquidity {
            pool_id: bytes(value, "poolId")?,
            sender: bytes(value, "sender")?,
            tick_lower: string_value(value, "tickLower")?,
            tick_upper: string_value(value, "tickUpper")?,
            liquidity_delta: string_value(value, "liquidityDelta")?,
            salt: bytes(value, "salt")?,
        }));
    }
    if let Some(value) = value.get("swap") {
        return Ok(pb::event::Payload::Swap(pb::Swap {
            pool_id: bytes(value, "poolId")?,
            sender: bytes(value, "sender")?,
            amount0: string_value(value, "amount0")?,
            amount1: string_value(value, "amount1")?,
            sqrt_price_x96: string_value(value, "sqrtPriceX96")?,
            liquidity: string_value(value, "liquidity")?,
            tick: string_value(value, "tick")?,
            fee: string_value(value, "fee")?,
        }));
    }
    if let Some(value) = value.get("subscription") {
        return Ok(pb::event::Payload::Subscription(pb::Subscription {
            token_id: string_value(value, "tokenId")?,
            subscriber: bytes(value, "subscriber")?,
        }));
    }
    if let Some(value) = value.get("unsubscription") {
        return Ok(pb::event::Payload::Unsubscription(pb::Unsubscription {
            token_id: string_value(value, "tokenId")?,
            subscriber: bytes(value, "subscriber")?,
        }));
    }
    if let Some(value) = value.get("transfer") {
        return Ok(pb::event::Payload::Transfer(pb::Transfer {
            from: bytes(value, "from")?,
            to: bytes(value, "to")?,
            token_id: string_value(value, "tokenId")?,
        }));
    }
    if let Some(value) = value.get("logCreatePrivateHook") {
        return Ok(pb::event::Payload::LogCreatePrivateHook(
            pb::LogCreatePrivateHook {
                hook: bytes(value, "hook")?,
                module: bytes(value, "module")?,
                salt: bytes(value, "salt")?,
            },
        ));
    }
    Err("unknown event payload".to_owned())
}

fn metadata(value: &Value) -> Result<pb::TokenMetadata, String> {
    Ok(pb::TokenMetadata {
        symbol: string_value(value, "symbol")?,
        name: string_value(value, "name")?,
        total_supply: string_value(value, "totalSupply")?,
        decimals: value.get("decimals").map(as_string).transpose()?,
    })
}

fn object<'a>(value: &'a Value, field: &str) -> Result<&'a Value, String> {
    value
        .get(field)
        .filter(|value| value.is_object())
        .ok_or_else(|| format!("missing object `{field}`"))
}

fn bytes(value: &Value, field: &str) -> Result<Vec<u8>, String> {
    let value = string_value(value, field)?;
    let value = value.strip_prefix("0x").unwrap_or(&value);
    if !value.len().is_multiple_of(2) {
        return Err(format!("odd-length bytes `{field}`"));
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|error| format!("invalid bytes `{field}`: {error}"))
        })
        .collect()
}

fn string_value(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .ok_or_else(|| format!("missing `{field}`"))
        .and_then(as_string)
}

fn as_string(value: &Value) -> Result<String, String> {
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Number(value) => Ok(value.to_string()),
        _ => Err("expected string or number".to_owned()),
    }
}

fn u64_value(value: &Value, field: &str) -> Result<u64, String> {
    string_value(value, field)?
        .parse()
        .map_err(|error| format!("invalid u64 `{field}`: {error}"))
}

fn i64_value(value: &Value, field: &str) -> Result<i64, String> {
    string_value(value, field)?
        .parse()
        .map_err(|error| format!("invalid i64 `{field}`: {error}"))
}

fn optional_u64(value: &Value, field: &str) -> Result<u64, String> {
    match value.get(field) {
        Some(value) => as_string(value)?
            .parse::<u64>()
            .map_err(|error| error.to_string()),
        None => Ok(0),
    }
}

fn optional_i64(value: &Value, field: &str) -> Result<i64, String> {
    match value.get(field) {
        Some(value) => as_string(value)?
            .parse::<i64>()
            .map_err(|error| error.to_string()),
        None => Ok(0),
    }
}

fn bigint(value: &Value, field: &str) -> Result<num_bigint::BigInt, String> {
    string_value(value, field)?
        .parse()
        .map_err(|error| format!("invalid BigInt `{field}`: {error}"))
}

fn optional_bigint(value: &Value, field: &str) -> Result<Option<num_bigint::BigInt>, String> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => as_string(value)?
            .parse()
            .map(Some)
            .map_err(|error| format!("invalid BigInt `{field}`: {error}")),
    }
}

fn decimal(value: &Value, field: &str) -> Result<GraphDecimal, String> {
    string_value(value, field)?
        .parse()
        .map_err(|error| format!("invalid BigDecimal `{field}`: {error}"))
}

fn optional_decimal(value: &Value, field: &str) -> Result<Option<GraphDecimal>, String> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => as_string(value)?
            .parse()
            .map(Some)
            .map_err(|error| format!("invalid BigDecimal `{field}`: {error}")),
    }
}

fn i32_field(value: &Value, field: &str) -> Result<i32, String> {
    string_value(value, field)?
        .parse()
        .map_err(|error| format!("invalid i32 `{field}`: {error}"))
}

fn optional_string(value: &Value, field: &str) -> Result<Option<String>, String> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => as_string(value).map(Some),
    }
}

fn strings(value: &Value, field: &str) -> Result<Vec<String>, String> {
    value
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("missing string array `{field}`"))
        .and_then(|values| values.iter().map(as_string).collect())
}

fn bytea(value: &Value, field: &str) -> Result<Vec<u8>, String> {
    let value = string_value(value, field)?;
    decode_bytea(&value, field)
}

fn optional_bytea(value: &Value, field: &str) -> Result<Option<Vec<u8>>, String> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => decode_bytea(&as_string(value)?, field).map(Some),
    }
}

fn decode_bytea(value: &str, field: &str) -> Result<Vec<u8>, String> {
    let value = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("\\x"))
        .unwrap_or(value);
    if !value.len().is_multiple_of(2) {
        return Err(format!("odd-length bytea `{field}`"));
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|error| format!("invalid bytea `{field}`: {error}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_arbitrary_precision_bigint_json_numbers() {
        let value: Value =
            serde_json::from_str(r#"{"total_supply":100000000000000000000000000000}"#).unwrap();
        assert_eq!(
            bigint(&value, "total_supply").unwrap().to_string(),
            "100000000000000000000000000000"
        );
    }

    #[test]
    fn omitted_protobuf_json_scalars_default_to_zero() {
        let value = serde_json::json!({});
        assert_eq!(optional_u64(&value, "index").unwrap(), 0);
        assert_eq!(optional_i64(&value, "timestampNanos").unwrap(), 0);
    }
}
