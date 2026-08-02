use crate::{
    decimal::GraphDecimal,
    entities::*,
    math,
    pb::pinax::uniswap::v4::base::v1 as pb,
    state::{EntityState, StateError},
};
use num_bigint::BigInt;
use num_traits::{One, ToPrimitive, Zero};
use std::str::FromStr;

const POOL_MANAGER: &str = "0x498581ff718922c3f8e6a244956af099b2652b2b";
const ADDRESS_ZERO: &str = "0x0000000000000000000000000000000000000000";
const WETH: &str = "0x4200000000000000000000000000000000000006";
const USDC: &str = "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913";
const ZORA: &str = "0x1111111111166b7fe7bd91427724b487980afc69";
const STABLE_POOL: &str = "0x90333bb05c258fe0dddb2840ef66f1a05165aa7dac6815d24e807cc6ebd943a0";
const CREATOR_HOOK: &str = "0xd61a675f8a0c67a73dc3b54fb7318b4d91409040";
const CONTENT_HOOK: &str = "0x9ea932730a7787000042e34390b8e435dd839040";

pub(crate) fn apply_event(state: &mut EntityState, event: &pb::Event) -> Result<(), StateError> {
    match event
        .payload
        .as_ref()
        .ok_or(StateError::Missing("payload"))?
    {
        pb::event::Payload::Initialize(value) => initialize(state, event, value),
        pb::event::Payload::ModifyLiquidity(value) => modify_liquidity(state, event, value),
        pb::event::Payload::Swap(value) => swap(state, event, value),
        pb::event::Payload::Subscription(value) => subscription(state, event, value),
        pb::event::Payload::Unsubscription(value) => unsubscription(state, event, value),
        pb::event::Payload::Transfer(value) => transfer(state, event, value),
        pb::event::Payload::LogCreatePrivateHook(value) => arrakis(state, event, value),
    }
}

fn initialize(
    state: &mut EntityState,
    event: &pb::Event,
    value: &pb::Initialize,
) -> Result<(), StateError> {
    let pool_id = hex(&value.pool_id);
    let mut pool_manager = if let Some(value) = state.pool_managers.get(POOL_MANAGER) {
        value.clone()
    } else {
        let pool_manager = PoolManager {
            id: POOL_MANAGER.to_owned(),
            pool_count: BigInt::zero(),
            tx_count: BigInt::zero(),
            total_volume_usd: zero(),
            total_volume_eth: zero(),
            total_fees_usd: zero(),
            total_fees_eth: zero(),
            untracked_volume_usd: zero(),
            total_value_locked_usd: zero(),
            total_value_locked_eth: zero(),
            total_value_locked_usd_untracked: zero(),
            total_value_locked_eth_untracked: zero(),
            owner: ADDRESS_ZERO.to_owned(),
        };
        state.save(
            EntityRecord::Bundle(Bundle {
                id: "1".to_owned(),
                eth_price_usd: zero(),
            }),
            event,
        )?;
        pool_manager
    };
    pool_manager.pool_count += BigInt::one();

    let token0_id = hex(&value.currency0);
    let token1_id = hex(&value.currency1);
    let mut token0 = match state.tokens.get(&token0_id) {
        Some(value) => value.clone(),
        None => match token_from_metadata(&token0_id, value.token0_metadata.as_ref())? {
            Some(value) => value,
            None => return Ok(()),
        },
    };
    let mut token1 = match state.tokens.get(&token1_id) {
        Some(value) => value.clone(),
        None => match token_from_metadata(&token1_id, value.token1_metadata.as_ref())? {
            Some(value) => value,
            None => return Ok(()),
        },
    };

    if whitelisted(&token0.id) {
        token1.whitelist_pools.push(pool_id.clone());
    }
    if whitelisted(&token1.id) {
        token0.whitelist_pools.push(pool_id.clone());
    }
    let hooks = hex(&value.hooks);
    if hooks == CONTENT_HOOK {
        if token0.whitelist_pools.is_empty() && !token1.whitelist_pools.is_empty() {
            token0.whitelist_pools.push(pool_id.clone());
        } else if token1.whitelist_pools.is_empty() && !token0.whitelist_pools.is_empty() {
            token1.whitelist_pools.push(pool_id.clone());
        }
    }

    let sqrt_price = parse_bigint("sqrt_price_x96", &value.sqrt_price_x96)?;
    let tick = parse_bigint("tick", &value.tick)?;
    let (token0_price, token1_price) = math::sqrt_price_x96_to_token_prices(
        &sqrt_price,
        &effective_decimals(&token0),
        &effective_decimals(&token1),
    );
    let block = block(event)?;
    let pool = Pool {
        id: pool_id.clone(),
        created_at_timestamp: block.timestamp_seconds.into(),
        created_at_block_number: block.number.into(),
        token0: token0.id.clone(),
        token1: token1.id.clone(),
        fee_tier: parse_bigint("fee", &value.fee)?,
        liquidity: BigInt::zero(),
        sqrt_price,
        token0_price,
        token1_price,
        tick: Some(tick),
        tick_spacing: parse_bigint("tick_spacing", &value.tick_spacing)?,
        observation_index: BigInt::zero(),
        volume_token0: zero(),
        volume_token1: zero(),
        volume_usd: zero(),
        untracked_volume_usd: zero(),
        fees_usd: zero(),
        tx_count: BigInt::zero(),
        collected_fees_token0: zero(),
        collected_fees_token1: zero(),
        collected_fees_usd: zero(),
        total_value_locked_token0: zero(),
        total_value_locked_token1: zero(),
        total_value_locked_eth: zero(),
        total_value_locked_usd: zero(),
        total_value_locked_usd_untracked: zero(),
        liquidity_provider_count: BigInt::zero(),
        hooks,
    };

    state.save(EntityRecord::Pool(pool), event)?;
    state.save(EntityRecord::Token(token0.clone()), event)?;
    state.save(EntityRecord::Token(token1.clone()), event)?;
    state.save(EntityRecord::PoolManager(pool_manager), event)?;

    let mut bundle = required_bundle(state)?;
    bundle.eth_price_usd = native_price_usd(state);
    state.save(EntityRecord::Bundle(bundle), event)?;
    update_pool_day(state, event, &pool_id)?;
    update_pool_hour(state, event, &pool_id)?;

    token1.derived_eth = find_native_per_token(state, &token1);
    token0.derived_eth = find_native_per_token(state, &token0);
    state.save(EntityRecord::Token(token0), event)?;
    state.save(EntityRecord::Token(token1), event)?;
    Ok(())
}

fn token_from_metadata(
    id: &str,
    metadata: Option<&pb::TokenMetadata>,
) -> Result<Option<Token>, StateError> {
    let metadata = metadata.ok_or_else(|| StateError::MissingMetadata(id.to_owned()))?;
    let Some(decimals) = metadata.decimals.as_ref() else {
        return Ok(None);
    };
    Ok(Some(Token {
        id: id.to_owned(),
        symbol: metadata.symbol.clone(),
        name: metadata.name.clone(),
        decimals: parse_bigint("token decimals", decimals)?,
        total_supply: parse_bigint("token total supply", &metadata.total_supply)?,
        volume: zero(),
        volume_usd: zero(),
        untracked_volume_usd: zero(),
        fees_usd: zero(),
        tx_count: BigInt::zero(),
        pool_count: BigInt::zero(),
        total_value_locked: zero(),
        total_value_locked_usd: zero(),
        total_value_locked_usd_untracked: zero(),
        derived_eth: zero(),
        whitelist_pools: Vec::new(),
    }))
}

fn subscription(
    state: &mut EntityState,
    event: &pb::Event,
    value: &pb::Subscription,
) -> Result<(), StateError> {
    let transaction = load_transaction(state, event)?;
    let token_id = parse_bigint("token_id", &value.token_id)?;
    let id = event_id(event)?;
    state.save(
        EntityRecord::Subscribe(Subscribe {
            id,
            token_id: token_id.clone(),
            address: hex(&value.subscriber),
            transaction: transaction.id,
            log_index: graph_log_index(event)?.into(),
            timestamp: transaction.timestamp,
            origin: hex(&transaction_ref(event)?.origin),
            position: token_id.to_string(),
        }),
        event,
    )
}

fn unsubscription(
    state: &mut EntityState,
    event: &pb::Event,
    value: &pb::Unsubscription,
) -> Result<(), StateError> {
    let transaction = load_transaction(state, event)?;
    let token_id = parse_bigint("token_id", &value.token_id)?;
    let id = event_id(event)?;
    state.save(
        EntityRecord::Unsubscribe(Unsubscribe {
            id,
            token_id: token_id.clone(),
            address: hex(&value.subscriber),
            transaction: transaction.id,
            log_index: graph_log_index(event)?.into(),
            timestamp: transaction.timestamp,
            origin: hex(&transaction_ref(event)?.origin),
            position: token_id.to_string(),
        }),
        event,
    )
}

fn transfer(
    state: &mut EntityState,
    event: &pb::Event,
    value: &pb::Transfer,
) -> Result<(), StateError> {
    let token_id = parse_bigint("token_id", &value.token_id)?;
    let id = token_id.to_string();
    let mut position = state.positions.get(&id).cloned().unwrap_or(Position {
        id: id.clone(),
        token_id: token_id.clone(),
        owner: String::new(),
        origin: hex(&transaction_ref(event)?.origin),
        created_at_timestamp: block(event)?.timestamp_seconds.into(),
    });
    position.owner = hex(&value.to);
    let transaction = load_transaction(state, event)?;
    let transfer = Transfer {
        id: event_id(event)?,
        token_id,
        from: hex(&value.from),
        to: hex(&value.to),
        transaction: transaction.id,
        log_index: graph_log_index(event)?.into(),
        timestamp: transaction.timestamp,
        origin: hex(&transaction_ref(event)?.origin),
        position: position.id.clone(),
    };
    state.save(EntityRecord::Position(position), event)?;
    state.save(EntityRecord::Transfer(transfer), event)
}

fn arrakis(
    state: &mut EntityState,
    event: &pb::Event,
    value: &pb::LogCreatePrivateHook,
) -> Result<(), StateError> {
    let block = block(event)?;
    state.save(
        EntityRecord::ArrakisHook(ArrakisHook {
            id: hex(&value.hook),
            module: value.module.clone(),
            salt: value.salt.clone(),
            created_at_timestamp: block.timestamp_seconds.into(),
            created_at_block_number: block.number.into(),
        }),
        event,
    )
}

fn load_transaction(state: &mut EntityState, event: &pb::Event) -> Result<Transaction, StateError> {
    let transaction_ref = transaction_ref(event)?;
    let block = block(event)?;
    let id = hex(&transaction_ref.hash);
    let transaction = Transaction {
        id,
        block_number: block.number.into(),
        timestamp: block.timestamp_seconds.into(),
        gas_used: BigInt::zero(),
        gas_price: parse_bigint("gas_price", &transaction_ref.gas_price)?,
    };
    state.save(EntityRecord::Transaction(transaction.clone()), event)?;
    Ok(transaction)
}

fn required_bundle(state: &EntityState) -> Result<Bundle, StateError> {
    state
        .bundles
        .get("1")
        .cloned()
        .ok_or_else(|| StateError::RequiredEntity {
            entity: "Bundle",
            id: "1".to_owned(),
        })
}

fn native_price_usd(state: &EntityState) -> GraphDecimal {
    state
        .pools
        .get(STABLE_POOL)
        .map(|pool| pool.token1_price.clone())
        .unwrap_or_else(zero)
}

fn find_native_per_token(state: &EntityState, token: &Token) -> GraphDecimal {
    if token.id == WETH || token.id == ADDRESS_ZERO {
        return GraphDecimal::one();
    }
    let bundle = match state.bundles.get("1") {
        Some(value) => value,
        None => return zero(),
    };
    if token.id == USDC {
        return math::safe_div(GraphDecimal::one(), bundle.eth_price_usd.clone());
    }

    let mut largest = zero();
    let mut price = zero();
    for pool_id in &token.whitelist_pools {
        let Some(pool) = state.pools.get(pool_id) else {
            continue;
        };
        if pool.liquidity <= BigInt::zero() {
            continue;
        }
        if pool.token0 == token.id {
            if let Some(other) = state.tokens.get(&pool.token1) {
                let locked = pool.total_value_locked_token1.clone() * other.derived_eth.clone();
                if locked > largest && locked > GraphDecimal::one() {
                    largest = locked;
                    price = pool.token1_price.clone() * other.derived_eth.clone();
                }
            }
        }
        if pool.token1 == token.id {
            if let Some(other) = state.tokens.get(&pool.token0) {
                let locked = pool.total_value_locked_token0.clone() * other.derived_eth.clone();
                if locked > largest && locked > GraphDecimal::one() {
                    largest = locked;
                    price = pool.token0_price.clone() * other.derived_eth.clone();
                }
            }
        }
    }
    price
}

fn whitelisted(id: &str) -> bool {
    matches!(id, WETH | USDC | ADDRESS_ZERO | ZORA)
}

fn effective_decimals(token: &Token) -> BigInt {
    if token.id == ADDRESS_ZERO {
        18.into()
    } else {
        token.decimals.clone()
    }
}

fn event_id(event: &pb::Event) -> Result<String, StateError> {
    Ok(format!(
        "{}-{}",
        hex(&transaction_ref(event)?.hash),
        graph_log_index(event)?
    ))
}

fn block(event: &pb::Event) -> Result<&pb::BlockRef, StateError> {
    event.block.as_ref().ok_or(StateError::Missing("block"))
}

fn transaction_ref(event: &pb::Event) -> Result<&pb::TransactionRef, StateError> {
    event
        .transaction
        .as_ref()
        .ok_or(StateError::Missing("transaction"))
}

fn graph_log_index(event: &pb::Event) -> Result<u32, StateError> {
    event
        .log
        .as_ref()
        .map(|value| value.block_log_index)
        .ok_or(StateError::Missing("log"))
}

fn parse_bigint(field: &'static str, value: &str) -> Result<BigInt, StateError> {
    BigInt::from_str(value).map_err(|_| StateError::InvalidInteger {
        field,
        value: value.to_owned(),
    })
}

fn hex(value: &[u8]) -> String {
    format!("0x{}", substreams::Hex(value))
}

fn zero() -> GraphDecimal {
    GraphDecimal::zero()
}

fn update_uniswap_day(
    state: &mut EntityState,
    event: &pb::Event,
) -> Result<UniswapDayData, StateError> {
    let manager =
        state
            .pool_managers
            .get(POOL_MANAGER)
            .ok_or_else(|| StateError::RequiredEntity {
                entity: "PoolManager",
                id: POOL_MANAGER.to_owned(),
            })?;
    let day = timestamp_i32(event)? / 86_400;
    let id = day.to_string();
    let mut value = state
        .uniswap_day_data
        .get(&id)
        .cloned()
        .unwrap_or(UniswapDayData {
            id,
            date: day * 86_400,
            volume_eth: zero(),
            volume_usd: zero(),
            volume_usd_untracked: zero(),
            fees_usd: zero(),
            tx_count: BigInt::zero(),
            tvl_usd: zero(),
        });
    value.tvl_usd = manager.total_value_locked_usd.clone();
    value.tx_count = manager.tx_count.clone();
    state.save(EntityRecord::UniswapDayData(value.clone()), event)?;
    Ok(value)
}

fn update_pool_day(
    state: &mut EntityState,
    event: &pb::Event,
    pool_id: &str,
) -> Result<PoolIntervalData, StateError> {
    update_pool_interval(state, event, pool_id, false)
}

fn update_pool_hour(
    state: &mut EntityState,
    event: &pb::Event,
    pool_id: &str,
) -> Result<PoolIntervalData, StateError> {
    update_pool_interval(state, event, pool_id, true)
}

fn update_pool_interval(
    state: &mut EntityState,
    event: &pb::Event,
    pool_id: &str,
    hourly: bool,
) -> Result<PoolIntervalData, StateError> {
    let pool = state
        .pools
        .get(pool_id)
        .cloned()
        .ok_or_else(|| StateError::RequiredEntity {
            entity: "Pool",
            id: pool_id.to_owned(),
        })?;
    let period = if hourly { 3_600 } else { 86_400 };
    let index = timestamp_i32(event)? / period;
    let id = format!("{}-{}", pool_id, index);
    let existing = if hourly {
        state.pool_hour_data.get(&id)
    } else {
        state.pool_day_data.get(&id)
    };
    let mut value = existing.cloned().unwrap_or(PoolIntervalData {
        id,
        start: index * period,
        pool: pool.id.clone(),
        liquidity: pool.liquidity.clone(),
        sqrt_price: pool.sqrt_price.clone(),
        token0_price: pool.token0_price.clone(),
        token1_price: pool.token1_price.clone(),
        tick: pool.tick.clone(),
        tvl_usd: pool.total_value_locked_usd.clone(),
        volume_token0: zero(),
        volume_token1: zero(),
        volume_usd: zero(),
        fees_usd: zero(),
        tx_count: BigInt::zero(),
        open: pool.token0_price.clone(),
        high: pool.token0_price.clone(),
        low: pool.token0_price.clone(),
        close: pool.token0_price.clone(),
    });
    if pool.token0_price > value.high {
        value.high = pool.token0_price.clone();
    }
    if pool.token0_price < value.low {
        value.low = pool.token0_price.clone();
    }
    value.liquidity = pool.liquidity;
    value.sqrt_price = pool.sqrt_price;
    value.token0_price = pool.token0_price.clone();
    value.token1_price = pool.token1_price;
    value.close = pool.token0_price;
    value.tick = pool.tick;
    value.tvl_usd = pool.total_value_locked_usd;
    value.tx_count += BigInt::one();
    let record = if hourly {
        EntityRecord::PoolHourData(value.clone())
    } else {
        EntityRecord::PoolDayData(value.clone())
    };
    state.save(record, event)?;
    Ok(value)
}

fn update_token_day(
    state: &mut EntityState,
    event: &pb::Event,
    token: &Token,
) -> Result<TokenIntervalData, StateError> {
    update_token_interval(state, event, token, false)
}

fn update_token_hour(
    state: &mut EntityState,
    event: &pb::Event,
    token: &Token,
) -> Result<TokenIntervalData, StateError> {
    update_token_interval(state, event, token, true)
}

fn update_token_interval(
    state: &mut EntityState,
    event: &pb::Event,
    token: &Token,
    hourly: bool,
) -> Result<TokenIntervalData, StateError> {
    let bundle = required_bundle(state)?;
    let period = if hourly { 3_600 } else { 86_400 };
    let index = timestamp_i32(event)? / period;
    let id = format!("{}-{}", token.id, index);
    let price = token.derived_eth.clone() * bundle.eth_price_usd;
    let existing = if hourly {
        state.token_hour_data.get(&id)
    } else {
        state.token_day_data.get(&id)
    };
    let mut value = existing.cloned().unwrap_or(TokenIntervalData {
        id,
        start: index * period,
        token: token.id.clone(),
        volume: zero(),
        volume_usd: zero(),
        untracked_volume_usd: zero(),
        total_value_locked: token.total_value_locked.clone(),
        total_value_locked_usd: token.total_value_locked_usd.clone(),
        price_usd: price.clone(),
        fees_usd: zero(),
        open: price.clone(),
        high: price.clone(),
        low: price.clone(),
        close: price.clone(),
    });
    if price > value.high {
        value.high = price.clone();
    }
    if price < value.low {
        value.low = price.clone();
    }
    value.close = price.clone();
    value.price_usd = price;
    value.total_value_locked = token.total_value_locked.clone();
    value.total_value_locked_usd = token.total_value_locked_usd.clone();
    let record = if hourly {
        EntityRecord::TokenHourData(value.clone())
    } else {
        EntityRecord::TokenDayData(value.clone())
    };
    state.save(record, event)?;
    Ok(value)
}

fn timestamp_i32(event: &pb::Event) -> Result<i32, StateError> {
    Ok(block(event)?.timestamp_seconds as i32)
}

fn calculate_amount_usd(
    amount0: GraphDecimal,
    amount1: GraphDecimal,
    token0_derived: GraphDecimal,
    token1_derived: GraphDecimal,
    native_price: GraphDecimal,
) -> GraphDecimal {
    amount0 * (token0_derived * native_price.clone()) + amount1 * (token1_derived * native_price)
}

fn tracked_amount_usd(
    amount0: GraphDecimal,
    token0: &Token,
    amount1: GraphDecimal,
    token1: &Token,
    bundle: &Bundle,
) -> GraphDecimal {
    let price0 = token0.derived_eth.clone() * bundle.eth_price_usd.clone();
    let price1 = token1.derived_eth.clone() * bundle.eth_price_usd.clone();
    match (whitelisted(&token0.id), whitelisted(&token1.id)) {
        (true, true) => amount0 * price0 + amount1 * price1,
        (true, false) => amount0 * price0 * GraphDecimal::from(2),
        (false, true) => amount1 * price1 * GraphDecimal::from(2),
        (false, false) => zero(),
    }
}

// Implemented below in the same order and with the same persisted-state reads
// as the deployed AssemblyScript mapping.
fn modify_liquidity(
    state: &mut EntityState,
    event: &pb::Event,
    value: &pb::ModifyLiquidity,
) -> Result<(), StateError> {
    modify_liquidity_impl(state, event, value)
}

fn modify_liquidity_impl(
    state: &mut EntityState,
    event: &pb::Event,
    value: &pb::ModifyLiquidity,
) -> Result<(), StateError> {
    let bundle = required_bundle(state)?;
    let pool_id = hex(&value.pool_id);
    let Some(mut pool) = state.pools.get(&pool_id).cloned() else {
        return Ok(());
    };
    let Some(mut manager) = state.pool_managers.get(POOL_MANAGER).cloned() else {
        return Ok(());
    };
    let Some(mut token0) = state.tokens.get(&pool.token0).cloned() else {
        return Ok(());
    };
    let Some(mut token1) = state.tokens.get(&pool.token1).cloned() else {
        return Ok(());
    };

    let current_tick = pool
        .tick
        .as_ref()
        .and_then(ToPrimitive::to_i32)
        .ok_or(StateError::Missing("pool.tick"))?;
    let lower = parse_i32("tick_lower", &value.tick_lower)?;
    let upper = parse_i32("tick_upper", &value.tick_upper)?;
    let liquidity = parse_bigint("liquidity_delta", &value.liquidity_delta)?;
    let (amount0_raw, amount1_raw) =
        math::liquidity_amounts(lower, upper, current_tick, &liquidity, &pool.sqrt_price);
    let amount0 = math::token_to_decimal(&amount0_raw, &token0.decimals);
    let amount1 = math::token_to_decimal(&amount1_raw, &token1.decimals);
    let amount_usd = calculate_amount_usd(
        amount0.clone(),
        amount1.clone(),
        token0.derived_eth.clone(),
        token1.derived_eth.clone(),
        bundle.eth_price_usd.clone(),
    );

    manager.total_value_locked_eth =
        manager.total_value_locked_eth - pool.total_value_locked_eth.clone();
    manager.tx_count += BigInt::one();

    token0.tx_count += BigInt::one();
    token0.total_value_locked = token0.total_value_locked + amount0.clone();
    token0.total_value_locked_usd = token0.total_value_locked.clone()
        * (token0.derived_eth.clone() * bundle.eth_price_usd.clone());
    token1.tx_count += BigInt::one();
    token1.total_value_locked = token1.total_value_locked + amount1.clone();
    token1.total_value_locked_usd = token1.total_value_locked.clone()
        * (token1.derived_eth.clone() * bundle.eth_price_usd.clone());

    pool.tx_count += BigInt::one();
    if lower <= current_tick && upper > current_tick {
        pool.liquidity += liquidity.clone();
    }
    pool.total_value_locked_token0 = pool.total_value_locked_token0 + amount0.clone();
    pool.total_value_locked_token1 = pool.total_value_locked_token1 + amount1.clone();
    pool.total_value_locked_eth = pool.total_value_locked_token0.clone()
        * token0.derived_eth.clone()
        + pool.total_value_locked_token1.clone() * token1.derived_eth.clone();
    pool.total_value_locked_usd =
        pool.total_value_locked_eth.clone() * bundle.eth_price_usd.clone();
    manager.total_value_locked_eth =
        manager.total_value_locked_eth + pool.total_value_locked_eth.clone();
    manager.total_value_locked_usd = manager.total_value_locked_eth.clone() * bundle.eth_price_usd;

    let transaction = load_transaction(state, event)?;
    let log_index = graph_log_index(event)?;
    let mutation = ModifyLiquidity {
        id: format!("{}-{}", transaction.id, log_index),
        transaction: transaction.id.clone(),
        timestamp: transaction.timestamp,
        pool: pool.id.clone(),
        token0: pool.token0.clone(),
        token1: pool.token1.clone(),
        sender: Some(value.sender.clone()),
        origin: transaction_ref(event)?.origin.clone(),
        amount: liquidity.clone(),
        amount0,
        amount1,
        amount_usd: Some(amount_usd),
        tick_lower: lower.into(),
        tick_upper: upper.into(),
        log_index: Some(log_index.into()),
    };

    let lower_id = format!("{}#{}", pool_id, lower);
    let upper_id = format!("{}#{}", pool_id, upper);
    let mut lower_tick = state
        .ticks
        .get(&lower_id)
        .cloned()
        .unwrap_or_else(|| create_tick(&lower_id, lower, &pool_id, event));
    let mut upper_tick = state
        .ticks
        .get(&upper_id)
        .cloned()
        .unwrap_or_else(|| create_tick(&upper_id, upper, &pool_id, event));
    lower_tick.liquidity_gross += liquidity.clone();
    lower_tick.liquidity_net += liquidity.clone();
    upper_tick.liquidity_gross += liquidity.clone();
    upper_tick.liquidity_net -= liquidity;
    state.save(EntityRecord::Tick(lower_tick), event)?;
    state.save(EntityRecord::Tick(upper_tick), event)?;

    update_uniswap_day(state, event)?;
    update_pool_day(state, event, &pool_id)?;
    update_pool_hour(state, event, &pool_id)?;
    update_token_day(state, event, &token0)?;
    update_token_day(state, event, &token1)?;
    update_token_hour(state, event, &token0)?;
    update_token_hour(state, event, &token1)?;

    state.save(EntityRecord::Token(token0), event)?;
    state.save(EntityRecord::Token(token1), event)?;
    state.save(EntityRecord::Pool(pool), event)?;
    state.save(EntityRecord::PoolManager(manager), event)?;
    state.save(EntityRecord::ModifyLiquidity(mutation), event)
}

fn create_tick(id: &str, index: i32, pool_id: &str, event: &pb::Event) -> Tick {
    let price0 =
        math::fast_exponentiation("1.0001".parse().expect("constant decimal is valid"), index);
    Tick {
        id: id.to_owned(),
        pool_address: Some(pool_id.to_owned()),
        tick_idx: index.into(),
        pool: pool_id.to_owned(),
        liquidity_gross: BigInt::zero(),
        liquidity_net: BigInt::zero(),
        price0: price0.clone(),
        price1: math::safe_div(GraphDecimal::one(), price0),
        created_at_timestamp: event
            .block
            .as_ref()
            .map(|value| value.timestamp_seconds.into())
            .unwrap_or_else(BigInt::zero),
        created_at_block_number: event
            .block
            .as_ref()
            .map(|value| value.number.into())
            .unwrap_or_else(BigInt::zero),
    }
}

fn swap(state: &mut EntityState, event: &pb::Event, value: &pb::Swap) -> Result<(), StateError> {
    swap_impl(state, event, value)
}

fn swap_impl(
    state: &mut EntityState,
    event: &pb::Event,
    value: &pb::Swap,
) -> Result<(), StateError> {
    let mut bundle = required_bundle(state)?;
    let mut manager = state
        .pool_managers
        .get(POOL_MANAGER)
        .cloned()
        .ok_or_else(|| StateError::RequiredEntity {
            entity: "PoolManager",
            id: POOL_MANAGER.to_owned(),
        })?;
    let pool_id = hex(&value.pool_id);
    let Some(mut pool) = state.pools.get(&pool_id).cloned() else {
        return Ok(());
    };
    let Some(mut token0) = state.tokens.get(&pool.token0).cloned() else {
        return Ok(());
    };
    let Some(mut token1) = state.tokens.get(&pool.token1).cloned() else {
        return Ok(());
    };

    if pool.hooks == CREATOR_HOOK {
        if whitelisted(&token1.id) && !token0.whitelist_pools.contains(&pool.id) {
            token0.whitelist_pools.push(pool.id.clone());
        }
        if whitelisted(&token0.id) && !token1.whitelist_pools.contains(&pool.id) {
            token1.whitelist_pools.push(pool.id.clone());
        }
    }
    if pool.hooks == CONTENT_HOOK {
        if token0.whitelist_pools.is_empty() && !token1.whitelist_pools.is_empty() {
            token0.whitelist_pools.push(pool.id.clone());
        } else if token1.whitelist_pools.is_empty() && !token0.whitelist_pools.is_empty() {
            token1.whitelist_pools.push(pool.id.clone());
        }
    }

    let negative_one = GraphDecimal::from(-1);
    let amount0 =
        math::token_to_decimal(&parse_bigint("amount0", &value.amount0)?, &token0.decimals)
            * negative_one.clone();
    let amount1 =
        math::token_to_decimal(&parse_bigint("amount1", &value.amount1)?, &token1.decimals)
            * negative_one;
    pool.fee_tier = parse_bigint("fee", &value.fee)?;
    let amount0_abs = amount0.abs();
    let amount1_abs = amount1.abs();
    let amount0_eth = amount0_abs.clone() * token0.derived_eth.clone();
    let amount1_eth = amount1_abs.clone() * token1.derived_eth.clone();
    let amount0_usd = amount0_eth * bundle.eth_price_usd.clone();
    let amount1_usd = amount1_eth * bundle.eth_price_usd.clone();
    let tracked_usd = tracked_amount_usd(
        amount0_abs.clone(),
        &token0,
        amount1_abs.clone(),
        &token1,
        &bundle,
    ) / GraphDecimal::from(2);
    let tracked_eth = math::safe_div(tracked_usd.clone(), bundle.eth_price_usd.clone());
    let untracked_usd = (amount0_usd + amount1_usd) / GraphDecimal::from(2);
    let fee_ratio = GraphDecimal::from_bigint(&pool.fee_tier) / GraphDecimal::from(1_000_000);
    let fees_eth = tracked_eth.clone() * fee_ratio.clone();
    let fees_usd = tracked_usd.clone() * fee_ratio;

    manager.tx_count += BigInt::one();
    manager.total_volume_eth = manager.total_volume_eth + tracked_eth.clone();
    manager.total_volume_usd = manager.total_volume_usd + tracked_usd.clone();
    manager.untracked_volume_usd = manager.untracked_volume_usd + untracked_usd.clone();
    manager.total_fees_eth = manager.total_fees_eth + fees_eth;
    manager.total_fees_usd = manager.total_fees_usd + fees_usd.clone();
    manager.total_value_locked_eth =
        manager.total_value_locked_eth - pool.total_value_locked_eth.clone();

    pool.volume_token0 = pool.volume_token0 + amount0_abs.clone();
    pool.volume_token1 = pool.volume_token1 + amount1_abs.clone();
    pool.volume_usd = pool.volume_usd + tracked_usd.clone();
    pool.untracked_volume_usd = pool.untracked_volume_usd + untracked_usd.clone();
    pool.fees_usd = pool.fees_usd + fees_usd.clone();
    pool.tx_count += BigInt::one();
    pool.liquidity = parse_bigint("liquidity", &value.liquidity)?;
    pool.tick = Some(parse_bigint("tick", &value.tick)?);
    pool.sqrt_price = parse_bigint("sqrt_price_x96", &value.sqrt_price_x96)?;
    pool.total_value_locked_token0 = pool.total_value_locked_token0 + amount0.clone();
    pool.total_value_locked_token1 = pool.total_value_locked_token1 + amount1.clone();

    token0.volume = token0.volume + amount0_abs.clone();
    token0.total_value_locked = token0.total_value_locked + amount0.clone();
    token0.volume_usd = token0.volume_usd + tracked_usd.clone();
    token0.untracked_volume_usd = token0.untracked_volume_usd + untracked_usd.clone();
    token0.fees_usd = token0.fees_usd + fees_usd.clone();
    token0.tx_count += BigInt::one();
    token1.volume = token1.volume + amount1_abs.clone();
    token1.total_value_locked = token1.total_value_locked + amount1.clone();
    token1.volume_usd = token1.volume_usd + tracked_usd.clone();
    token1.untracked_volume_usd = token1.untracked_volume_usd + untracked_usd;
    token1.fees_usd = token1.fees_usd + fees_usd.clone();
    token1.tx_count += BigInt::one();

    let prices = math::sqrt_price_x96_to_token_prices(
        &pool.sqrt_price,
        &effective_decimals(&token0),
        &effective_decimals(&token1),
    );
    pool.token0_price = prices.0;
    pool.token1_price = prices.1;
    bundle.eth_price_usd = native_price_usd(state);
    state.save(EntityRecord::Bundle(bundle.clone()), event)?;
    token0.derived_eth = find_native_per_token(state, &token0);
    token1.derived_eth = find_native_per_token(state, &token1);

    pool.total_value_locked_eth = pool.total_value_locked_token0.clone()
        * token0.derived_eth.clone()
        + pool.total_value_locked_token1.clone() * token1.derived_eth.clone();
    pool.total_value_locked_usd =
        pool.total_value_locked_eth.clone() * bundle.eth_price_usd.clone();
    manager.total_value_locked_eth =
        manager.total_value_locked_eth + pool.total_value_locked_eth.clone();
    manager.total_value_locked_usd =
        manager.total_value_locked_eth.clone() * bundle.eth_price_usd.clone();
    token0.total_value_locked_usd = token0.total_value_locked.clone()
        * token0.derived_eth.clone()
        * bundle.eth_price_usd.clone();
    token1.total_value_locked_usd =
        token1.total_value_locked.clone() * token1.derived_eth.clone() * bundle.eth_price_usd;

    let transaction = load_transaction(state, event)?;
    let log_index = graph_log_index(event)?;
    let swap = Swap {
        id: format!("{}-{}", transaction.id, log_index),
        transaction: transaction.id,
        timestamp: transaction.timestamp,
        pool: pool.id.clone(),
        token0: pool.token0.clone(),
        token1: pool.token1.clone(),
        sender: value.sender.clone(),
        origin: transaction_ref(event)?.origin.clone(),
        amount0,
        amount1,
        amount_usd: tracked_usd.clone(),
        sqrt_price_x96: pool.sqrt_price.clone(),
        tick: pool.tick.clone().expect("swap always assigns tick"),
        log_index: Some(log_index.into()),
    };

    let mut uniswap_day = update_uniswap_day(state, event)?;
    let mut pool_day = update_pool_day(state, event, &pool_id)?;
    let mut pool_hour = update_pool_hour(state, event, &pool_id)?;
    let mut token0_day = update_token_day(state, event, &token0)?;
    let mut token1_day = update_token_day(state, event, &token1)?;
    let mut token0_hour = update_token_hour(state, event, &token0)?;
    let mut token1_hour = update_token_hour(state, event, &token1)?;

    uniswap_day.volume_eth = uniswap_day.volume_eth + tracked_eth;
    uniswap_day.volume_usd = uniswap_day.volume_usd + tracked_usd.clone();
    uniswap_day.fees_usd = uniswap_day.fees_usd + fees_usd.clone();
    for value in [&mut pool_day, &mut pool_hour] {
        value.volume_usd = value.volume_usd.clone() + tracked_usd.clone();
        value.volume_token0 = value.volume_token0.clone() + amount0_abs.clone();
        value.volume_token1 = value.volume_token1.clone() + amount1_abs.clone();
        value.fees_usd = value.fees_usd.clone() + fees_usd.clone();
    }
    for value in [&mut token0_day, &mut token0_hour] {
        value.volume = value.volume.clone() + amount0_abs.clone();
        value.volume_usd = value.volume_usd.clone() + tracked_usd.clone();
        value.untracked_volume_usd = value.untracked_volume_usd.clone() + tracked_usd.clone();
        value.fees_usd = value.fees_usd.clone() + fees_usd.clone();
    }
    for value in [&mut token1_day, &mut token1_hour] {
        value.volume = value.volume.clone() + amount1_abs.clone();
        value.volume_usd = value.volume_usd.clone() + tracked_usd.clone();
        value.untracked_volume_usd = value.untracked_volume_usd.clone() + tracked_usd.clone();
        value.fees_usd = value.fees_usd.clone() + fees_usd.clone();
    }

    state.save(EntityRecord::Swap(swap), event)?;
    state.save(EntityRecord::TokenDayData(token0_day), event)?;
    state.save(EntityRecord::TokenDayData(token1_day), event)?;
    state.save(EntityRecord::UniswapDayData(uniswap_day), event)?;
    state.save(EntityRecord::PoolDayData(pool_day), event)?;
    state.save(EntityRecord::PoolHourData(pool_hour), event)?;
    state.save(EntityRecord::TokenHourData(token0_hour), event)?;
    state.save(EntityRecord::TokenHourData(token1_hour), event)?;
    state.save(EntityRecord::PoolManager(manager), event)?;
    state.save(EntityRecord::Pool(pool), event)?;
    state.save(EntityRecord::Token(token0), event)?;
    state.save(EntityRecord::Token(token1), event)
}

fn parse_i32(field: &'static str, value: &str) -> Result<i32, StateError> {
    value.parse().map_err(|_| StateError::InvalidInteger {
        field,
        value: value.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const Q96: &str = "79228162514264337593543950336";

    #[test]
    fn reconstructs_all_eighteen_schema_entities_deterministically() {
        let events = pb::Events {
            events: vec![
                event(
                    1,
                    pb::event::Payload::Initialize(pb::Initialize {
                        pool_id: vec![0x11; 32],
                        currency0: vec![0; 20],
                        currency1: vec![0x22; 20],
                        fee: "3000".to_owned(),
                        tick_spacing: "60".to_owned(),
                        hooks: vec![0; 20],
                        sqrt_price_x96: Q96.to_owned(),
                        tick: "0".to_owned(),
                        token0_metadata: Some(metadata("ETH", "Ethereum", 18)),
                        token1_metadata: Some(metadata("TKN", "Token", 18)),
                    }),
                ),
                event(
                    2,
                    pb::event::Payload::ModifyLiquidity(pb::ModifyLiquidity {
                        pool_id: vec![0x11; 32],
                        sender: vec![0x33; 20],
                        tick_lower: "-60".to_owned(),
                        tick_upper: "60".to_owned(),
                        liquidity_delta: "1000000000000000000".to_owned(),
                        salt: vec![0; 32],
                    }),
                ),
                event(
                    3,
                    pb::event::Payload::Swap(pb::Swap {
                        pool_id: vec![0x11; 32],
                        sender: vec![0x44; 20],
                        amount0: "-1000000000000000000".to_owned(),
                        amount1: "1000000000000000000".to_owned(),
                        sqrt_price_x96: Q96.to_owned(),
                        liquidity: "1000000000000000000".to_owned(),
                        tick: "0".to_owned(),
                        fee: "3000".to_owned(),
                    }),
                ),
                event(
                    4,
                    pb::event::Payload::Transfer(pb::Transfer {
                        from: vec![0; 20],
                        to: vec![0x55; 20],
                        token_id: "7".to_owned(),
                    }),
                ),
                event(
                    5,
                    pb::event::Payload::Subscription(pb::Subscription {
                        token_id: "7".to_owned(),
                        subscriber: vec![0x66; 20],
                    }),
                ),
                event(
                    6,
                    pb::event::Payload::Unsubscription(pb::Unsubscription {
                        token_id: "7".to_owned(),
                        subscriber: vec![0x66; 20],
                    }),
                ),
                event(
                    7,
                    pb::event::Payload::LogCreatePrivateHook(pb::LogCreatePrivateHook {
                        hook: vec![0x77; 20],
                        module: vec![0x88; 20],
                        salt: vec![0x99; 32],
                    }),
                ),
            ],
        };

        let mut first = EntityState::default();
        first.apply(&events).unwrap();
        let mut second = EntityState::default();
        second.apply(&events).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.entity_count(), 26); // interval/tick/transaction cardinality exceeds type count
        assert!(!first.pool_managers.is_empty());
        assert!(!first.bundles.is_empty());
        assert!(!first.tokens.is_empty());
        assert!(!first.pools.is_empty());
        assert!(!first.ticks.is_empty());
        assert!(!first.transactions.is_empty());
        assert!(!first.swaps.is_empty());
        assert!(!first.modify_liquidities.is_empty());
        assert!(!first.uniswap_day_data.is_empty());
        assert!(!first.pool_day_data.is_empty());
        assert!(!first.pool_hour_data.is_empty());
        assert!(!first.token_day_data.is_empty());
        assert!(!first.token_hour_data.is_empty());
        assert!(!first.positions.is_empty());
        assert!(!first.subscriptions.is_empty());
        assert!(!first.unsubscriptions.is_empty());
        assert!(!first.transfers.is_empty());
        assert!(!first.arrakis_hooks.is_empty());
    }

    #[test]
    fn initialize_preserves_the_deployed_save_order() {
        let initialize = event(
            1,
            pb::event::Payload::Initialize(pb::Initialize {
                pool_id: vec![0x11; 32],
                currency0: vec![0; 20],
                currency1: vec![0x22; 20],
                fee: "3000".to_owned(),
                tick_spacing: "60".to_owned(),
                hooks: vec![0; 20],
                sqrt_price_x96: Q96.to_owned(),
                tick: "0".to_owned(),
                token0_metadata: Some(metadata("ETH", "Ethereum", 18)),
                token1_metadata: Some(metadata("TKN", "Token", 18)),
            }),
        );
        let mut state = EntityState::default();
        state
            .apply(&pb::Events {
                events: vec![initialize],
            })
            .unwrap();
        assert_eq!(
            state
                .changes
                .iter()
                .map(|change| change.entity.entity_type())
                .collect::<Vec<_>>(),
            [
                "Bundle",
                "Pool",
                "Token",
                "Token",
                "PoolManager",
                "Bundle",
                "PoolDayData",
                "PoolHourData",
                "Token",
                "Token",
            ]
        );
    }

    #[test]
    fn a_missing_decimal_bails_after_the_initial_bundle_write() {
        let mut event = event(
            1,
            pb::event::Payload::Initialize(pb::Initialize {
                pool_id: vec![0x11; 32],
                currency0: vec![0x22; 20],
                currency1: vec![0x33; 20],
                fee: "3000".to_owned(),
                tick_spacing: "60".to_owned(),
                hooks: vec![0; 20],
                sqrt_price_x96: Q96.to_owned(),
                tick: "0".to_owned(),
                token0_metadata: Some(metadata("BAD", "Bad", 18)),
                token1_metadata: Some(metadata("BAD", "Bad", 18)),
            }),
        );
        let pb::event::Payload::Initialize(value) = event.payload.as_mut().unwrap() else {
            unreachable!()
        };
        value.token0_metadata.as_mut().unwrap().decimals = None;
        let mut state = EntityState::default();
        state
            .apply(&pb::Events {
                events: vec![event],
            })
            .unwrap();
        assert_eq!(state.changes.len(), 1);
        assert_eq!(state.changes[0].entity.entity_type(), "Bundle");
        assert!(state.pool_managers.is_empty());
    }

    #[test]
    fn matches_shared_initialize_fields_from_root_oracle() {
        // Captured from graphman dump at Base block 25,352,561 / hash
        // 0x309c5d...30f8. The canonical dump is byte-determinism tested by
        // oracle/verify-determinism.sh. The graft parent uses different Base
        // whitelist configuration, so this is deliberately not a complete
        // child-deployment entity snapshot comparison.
        let event = pb::Event {
            block: Some(pb::BlockRef {
                number: 25_352_561,
                hash: decode("309c5dd0325515927305c93df7bfa9e3cef7adb15cf1ae3f45f227db15d830f8"),
                parent_hash: decode(
                    "156f1a2d5ef962e52058ceebf8a94ecbb7c2b416196764c3a1a2acd67a44d412",
                ),
                timestamp_seconds: 1_737_494_469,
                timestamp_nanos: 0,
            }),
            transaction: Some(pb::TransactionRef {
                index: 107,
                hash: decode("4553d64b518439bdda2d2f1eafb7d4dd5b76efef6769320f0672b93e488f81a8"),
                origin: decode("368ba1315c483719eecddb1b80fafa3bbbf5b53e"),
                to: decode("498581ff718922c3f8e6a244956af099b2652b2b"),
                gas_price: "1384165".to_owned(),
            }),
            log: Some(pb::LogRef {
                transaction_log_index: 0,
                block_log_index: 306,
                ordinal: 18_091,
                address: decode("498581ff718922c3f8e6a244956af099b2652b2b"),
                graph_node_trigger_order: 306,
            }),
            source: pb::DataSource::PoolManager as i32,
            payload: Some(pb::event::Payload::Initialize(pb::Initialize {
                pool_id: decode("7af84d60777413f90cc511a83cf702b128bc885f84d6ca8be4b60063328d907a"),
                currency0: vec![0; 20],
                currency1: decode("000000000d564d5be76f7f0d28fe52605afc7cf8"),
                fee: "0".to_owned(),
                tick_spacing: "60".to_owned(),
                hooks: decode("9e433f32bb5481a9ca7dff5b3af74a7ed041a888"),
                sqrt_price_x96: Q96.to_owned(),
                tick: "0".to_owned(),
                token0_metadata: Some(pb::TokenMetadata {
                    symbol: "ETH".to_owned(),
                    name: "Ethereum".to_owned(),
                    total_supply: "0".to_owned(),
                    decimals: Some("18".to_owned()),
                }),
                token1_metadata: Some(pb::TokenMetadata {
                    symbol: "flETH".to_owned(),
                    name: "flETH".to_owned(),
                    total_supply: "0".to_owned(),
                    decimals: Some("18".to_owned()),
                }),
            })),
        };
        let mut state = EntityState::default();
        state
            .apply(&pb::Events {
                events: vec![event],
            })
            .unwrap();

        let pool_id = "0x7af84d60777413f90cc511a83cf702b128bc885f84d6ca8be4b60063328d907a";
        let pool = &state.pools[pool_id];
        assert_eq!(pool.created_at_timestamp.to_string(), "1737494469");
        assert_eq!(pool.created_at_block_number.to_string(), "25352561");
        assert_eq!(pool.token0_price.to_string(), "1");
        assert_eq!(pool.token1_price.to_string(), "1");
        assert_eq!(pool.sqrt_price.to_string(), Q96);
        assert_eq!(pool.hooks, "0x9e433f32bb5481a9ca7dff5b3af74a7ed041a888");
        assert_eq!(state.pool_managers[POOL_MANAGER].pool_count, BigInt::one());
        assert_eq!(state.bundles["1"].eth_price_usd.to_string(), "0");
        assert_eq!(state.tokens[ADDRESS_ZERO].derived_eth.to_string(), "1");
        assert_eq!(
            state.tokens["0x000000000d564d5be76f7f0d28fe52605afc7cf8"].symbol,
            "flETH"
        );
        let day = &state.pool_day_data[&format!("{pool_id}-20109")];
        assert_eq!(day.start, 1_737_417_600);
        assert_eq!(day.tx_count, BigInt::one());
        let hour = &state.pool_hour_data[&format!("{pool_id}-482637")];
        assert_eq!(hour.start, 1_737_493_200);
        assert_eq!(hour.tx_count, BigInt::one());
    }

    fn metadata(symbol: &str, name: &str, decimals: u8) -> pb::TokenMetadata {
        pb::TokenMetadata {
            symbol: symbol.to_owned(),
            name: name.to_owned(),
            total_supply: "1000000000000000000000000".to_owned(),
            decimals: Some(decimals.to_string()),
        }
    }

    fn event(index: u32, payload: pb::event::Payload) -> pb::Event {
        pb::Event {
            block: Some(pb::BlockRef {
                number: 26_990_279,
                hash: vec![0xaa; 32],
                parent_hash: vec![0xbb; 32],
                timestamp_seconds: 1_735_689_600,
                timestamp_nanos: 0,
            }),
            transaction: Some(pb::TransactionRef {
                index,
                hash: vec![index as u8; 32],
                origin: vec![0xcc; 20],
                to: vec![0xdd; 20],
                gas_price: "1000000".to_owned(),
            }),
            log: Some(pb::LogRef {
                transaction_log_index: 0,
                block_log_index: index,
                ordinal: u64::from(index),
                address: vec![0xee; 20],
                graph_node_trigger_order: u64::from(index),
            }),
            source: pb::DataSource::PoolManager as i32,
            payload: Some(payload),
        }
    }

    fn decode(value: &str) -> Vec<u8> {
        (0..value.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&value[index..index + 2], 16).unwrap())
            .collect()
    }
}
