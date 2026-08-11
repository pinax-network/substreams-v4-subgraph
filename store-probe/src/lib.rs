#![allow(clippy::not_unsafe_ptr_arg_deref)]
#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use num_bigint::BigInt as MathBigInt;
use num_traits::{One, Signed, Zero};
use substreams::{
    scalar::BigInt as StoreBigInt,
    store::{DeltaBigInt, Deltas, StoreGet, StoreGetBigInt},
};

mod decimal;

mod pb {
    pub mod events {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src/pb/pinax.uniswap.v4.base.v1.rs"
        ));
    }

    pub mod probe {
        #[derive(Clone, PartialEq, prost::Message)]
        pub struct StoreProbe {
            #[prost(message, repeated, tag = "1")]
            pub pools: Vec<PoolState>,
            #[prost(message, repeated, tag = "2")]
            pub tick_deltas: Vec<TickDelta>,
        }

        #[derive(Clone, PartialEq, prost::Message)]
        pub struct PoolState {
            #[prost(uint64, tag = "1")]
            pub block_number: u64,
            #[prost(uint64, tag = "2")]
            pub trigger_order: u64,
            #[prost(uint64, tag = "3")]
            pub ordinal: u64,
            #[prost(string, tag = "4")]
            pub pool_id: String,
            #[prost(string, tag = "5")]
            pub tick: String,
            #[prost(string, tag = "6")]
            pub liquidity: String,
            #[prost(string, tag = "7")]
            pub transaction_count: String,
        }

        #[derive(Clone, PartialEq, prost::Message)]
        pub struct TickDelta {
            #[prost(uint64, tag = "1")]
            pub ordinal: u64,
            #[prost(string, tag = "2")]
            pub key: String,
            #[prost(string, tag = "3")]
            pub old_value: String,
            #[prost(string, tag = "4")]
            pub new_value: String,
            #[prost(int32, tag = "5")]
            pub operation: i32,
        }

        #[derive(Clone, PartialEq, prost::Message)]
        pub struct ReducerInputs {
            #[prost(message, repeated, tag = "1")]
            pub events: Vec<ReducerEvent>,
            #[prost(uint32, tag = "2")]
            pub state_version: u32,
        }

        #[derive(Clone, PartialEq, prost::Message)]
        pub struct ReducerEvent {
            #[prost(message, optional, tag = "1")]
            pub event: Option<super::events::Event>,
            #[prost(message, optional, tag = "2")]
            pub pool: Option<PoolPrimitiveState>,
            #[prost(message, repeated, tag = "3")]
            pub ticks: Vec<TickPrimitiveState>,
            #[prost(message, optional, tag = "4")]
            pub modify_liquidity: Option<ModifyLiquidityComputation>,
            #[prost(message, optional, tag = "5")]
            pub token_amounts: Option<TokenAmountComputation>,
        }

        #[derive(Clone, PartialEq, prost::Message)]
        pub struct PoolPrimitiveState {
            #[prost(bytes = "vec", tag = "1")]
            pub tick_before: Vec<u8>,
            #[prost(bytes = "vec", tag = "2")]
            pub tick_after: Vec<u8>,
            #[prost(bytes = "vec", tag = "3")]
            pub sqrt_price_before: Vec<u8>,
            #[prost(bytes = "vec", tag = "4")]
            pub sqrt_price_after: Vec<u8>,
            #[prost(bytes = "vec", tag = "5")]
            pub liquidity_before: Vec<u8>,
            #[prost(bytes = "vec", tag = "6")]
            pub liquidity_after: Vec<u8>,
            #[prost(bytes = "vec", tag = "7")]
            pub transaction_count_before: Vec<u8>,
            #[prost(bytes = "vec", tag = "8")]
            pub transaction_count_after: Vec<u8>,
            #[prost(string, tag = "9")]
            pub token0_price_after: String,
            #[prost(string, tag = "10")]
            pub token1_price_after: String,
        }

        #[derive(Clone, PartialEq, prost::Message)]
        pub struct ModifyLiquidityComputation {
            #[prost(bytes = "vec", tag = "1")]
            pub amount0_raw: Vec<u8>,
            #[prost(bytes = "vec", tag = "2")]
            pub amount1_raw: Vec<u8>,
        }

        #[derive(Clone, PartialEq, prost::Message)]
        pub struct TokenAmountComputation {
            #[prost(string, tag = "1")]
            pub amount0: String,
            #[prost(string, tag = "2")]
            pub amount1: String,
        }

        #[derive(Clone, PartialEq, prost::Message)]
        pub struct TickPrimitiveState {
            #[prost(bytes = "vec", tag = "2")]
            pub liquidity_gross_before: Vec<u8>,
            #[prost(bytes = "vec", tag = "3")]
            pub liquidity_gross_after: Vec<u8>,
            #[prost(bytes = "vec", tag = "4")]
            pub liquidity_net_before: Vec<u8>,
            #[prost(bytes = "vec", tag = "5")]
            pub liquidity_net_after: Vec<u8>,
        }
    }
}

use pb::{
    events::{event, Event, Events},
    probe::{
        ModifyLiquidityComputation, PoolPrimitiveState, PoolState, ReducerEvent, ReducerInputs,
        StoreProbe, TickDelta, TickPrimitiveState, TokenAmountComputation,
    },
};

type TickDeltaValues = HashMap<(u64, String), (Vec<u8>, Vec<u8>)>;

#[substreams::handlers::map]
pub fn map_store_probe(
    events: Events,
    pool_ticks: StoreGetBigInt,
    pool_liquidity: StoreGetBigInt,
    pool_transaction_counts: StoreGetBigInt,
    tick_deltas: Deltas<DeltaBigInt>,
) -> StoreProbe {
    let pools = events
        .events
        .iter()
        .filter_map(|event| {
            let pool_id = event_pool(event)?;
            let ord = ordinal(event);
            let log = event.log.as_ref();
            Some(PoolState {
                block_number: event.block.as_ref().map_or(0, |block| block.number),
                trigger_order: log.map_or(0, |value| value.graph_node_trigger_order),
                ordinal: ord,
                tick: pool_ticks
                    .get_at(ord, &pool_id)
                    .map_or_else(String::new, |value| value.to_string()),
                liquidity: pool_liquidity
                    .get_at(ord, &pool_id)
                    .map_or_else(String::new, |value| value.to_string()),
                transaction_count: pool_transaction_counts
                    .get_at(ord, &pool_id)
                    .map_or_else(String::new, |value| value.to_string()),
                pool_id,
            })
        })
        .collect();
    let tick_deltas = tick_deltas
        .into_iter()
        .map(|delta| TickDelta {
            ordinal: delta.ordinal,
            key: delta.key,
            old_value: delta.old_value.to_string(),
            new_value: delta.new_value.to_string(),
            operation: delta.operation as i32,
        })
        .collect();

    StoreProbe { pools, tick_deltas }
}

#[substreams::handlers::map]
pub fn map_reducer_inputs(events: Events, tick_deltas: Deltas<DeltaBigInt>) -> ReducerInputs {
    let tick_values = tick_deltas
        .into_iter()
        .map(|delta| {
            (
                (delta.ordinal, delta.key),
                (
                    delta.old_value.to_signed_bytes_be(),
                    delta.new_value.to_signed_bytes_be(),
                ),
            )
        })
        .collect::<TickDeltaValues>();

    ReducerInputs {
        events: events
            .events
            .iter()
            .map(|event| reducer_event(event, &tick_values))
            .collect(),
        state_version: 0,
    }
}

/// Version-three boundary: Store-owned Pool/Tick state plus cached event-local
/// integer and Graph Decimal math. Recursive Graph state remains in the ordered
/// native reducer.
#[allow(clippy::too_many_arguments)]
#[substreams::handlers::map]
pub fn map_store_state_inputs(
    events: Events,
    pool_ticks: StoreGetBigInt,
    pool_sqrt_prices: StoreGetBigInt,
    pool_liquidity: StoreGetBigInt,
    pool_transaction_counts: StoreGetBigInt,
    pool_token_decimals: StoreGetBigInt,
    tick_deltas_00: Deltas<DeltaBigInt>,
    tick_deltas_01: Deltas<DeltaBigInt>,
    tick_deltas_02: Deltas<DeltaBigInt>,
    tick_deltas_03: Deltas<DeltaBigInt>,
    tick_deltas_04: Deltas<DeltaBigInt>,
    tick_deltas_05: Deltas<DeltaBigInt>,
    tick_deltas_06: Deltas<DeltaBigInt>,
    tick_deltas_07: Deltas<DeltaBigInt>,
    tick_deltas_08: Deltas<DeltaBigInt>,
    tick_deltas_09: Deltas<DeltaBigInt>,
    tick_deltas_10: Deltas<DeltaBigInt>,
    tick_deltas_11: Deltas<DeltaBigInt>,
    tick_deltas_12: Deltas<DeltaBigInt>,
    tick_deltas_13: Deltas<DeltaBigInt>,
    tick_deltas_14: Deltas<DeltaBigInt>,
    tick_deltas_15: Deltas<DeltaBigInt>,
) -> ReducerInputs {
    let mut tick_values = TickDeltaValues::new();
    for tick_deltas in [
        tick_deltas_00,
        tick_deltas_01,
        tick_deltas_02,
        tick_deltas_03,
        tick_deltas_04,
        tick_deltas_05,
        tick_deltas_06,
        tick_deltas_07,
        tick_deltas_08,
        tick_deltas_09,
        tick_deltas_10,
        tick_deltas_11,
        tick_deltas_12,
        tick_deltas_13,
        tick_deltas_14,
        tick_deltas_15,
    ] {
        tick_values.extend(tick_delta_values(tick_deltas));
    }
    ReducerInputs {
        events: events
            .events
            .iter()
            .map(|event| {
                state_reducer_event(
                    event,
                    &pool_ticks,
                    &pool_sqrt_prices,
                    &pool_liquidity,
                    &pool_transaction_counts,
                    &pool_token_decimals,
                    &tick_values,
                )
            })
            .collect(),
        state_version: 3,
    }
}

fn tick_delta_values(tick_deltas: Deltas<DeltaBigInt>) -> TickDeltaValues {
    tick_deltas
        .into_iter()
        .map(|delta| {
            (
                (delta.ordinal, delta.key),
                (
                    delta.old_value.to_signed_bytes_be(),
                    delta.new_value.to_signed_bytes_be(),
                ),
            )
        })
        .collect()
}

fn state_reducer_event(
    event: &Event,
    pool_ticks: &StoreGetBigInt,
    pool_sqrt_prices: &StoreGetBigInt,
    pool_liquidity: &StoreGetBigInt,
    pool_transaction_counts: &StoreGetBigInt,
    pool_token_decimals: &StoreGetBigInt,
    tick_values: &TickDeltaValues,
) -> ReducerEvent {
    let mut output = reducer_event(event, tick_values);
    let Some(pool_id) = event_pool(event) else {
        return output;
    };
    let ord = ordinal(event);
    let (tick_before, tick_after) = store_values(pool_ticks, ord, &pool_id);
    let (sqrt_before, sqrt_after) = store_values(pool_sqrt_prices, ord, &pool_id);
    let (liquidity_before, liquidity_after) = store_values(pool_liquidity, ord, &pool_id);
    let (transactions_before, transactions_after) =
        store_values(pool_transaction_counts, ord, &pool_id);
    let token0_decimals = pool_token_decimals.get_at(ord, format!("0:{pool_id}"));
    let token1_decimals = pool_token_decimals.get_at(ord, format!("1:{pool_id}"));
    output.modify_liquidity = modify_liquidity_computation(event, &tick_before, &sqrt_before);
    output.token_amounts = token_amount_computation(
        event,
        output.modify_liquidity.as_ref(),
        &token0_decimals,
        &token1_decimals,
    );
    let (token0_price_after, token1_price_after) =
        pool_prices(event, &sqrt_after, &token0_decimals, &token1_decimals).unwrap_or_default();
    output.pool = Some(PoolPrimitiveState {
        tick_before: store_bytes(tick_before),
        tick_after: store_bytes(tick_after),
        sqrt_price_before: store_bytes(sqrt_before),
        sqrt_price_after: store_bytes(sqrt_after),
        liquidity_before: store_bytes(liquidity_before),
        liquidity_after: store_bytes(liquidity_after),
        transaction_count_before: store_bytes(transactions_before),
        transaction_count_after: store_bytes(transactions_after),
        token0_price_after,
        token1_price_after,
    });
    output
}

fn store_values(
    store: &StoreGetBigInt,
    ordinal: u64,
    key: &str,
) -> (Option<StoreBigInt>, Option<StoreBigInt>) {
    (
        store.get_at(ordinal.saturating_sub(1), key),
        store.get_at(ordinal, key),
    )
}

fn store_bytes(value: Option<StoreBigInt>) -> Vec<u8> {
    value.map_or_else(Vec::new, |value| value.to_signed_bytes_be())
}

fn modify_liquidity_computation(
    event: &Event,
    current_tick: &Option<StoreBigInt>,
    current_sqrt_price: &Option<StoreBigInt>,
) -> Option<ModifyLiquidityComputation> {
    let event::Payload::ModifyLiquidity(value) = event.payload.as_ref()? else {
        return None;
    };
    let tick = current_tick.as_ref()?.to_i32();
    let sqrt_price =
        MathBigInt::from_signed_bytes_be(&current_sqrt_price.as_ref()?.to_signed_bytes_be());
    let liquidity = value.liquidity_delta.parse::<MathBigInt>().ok()?;
    let lower = value.tick_lower.parse::<i32>().ok()?;
    let upper = value.tick_upper.parse::<i32>().ok()?;
    let (amount0, amount1) = liquidity_amounts(lower, upper, tick, &liquidity, &sqrt_price);
    Some(ModifyLiquidityComputation {
        amount0_raw: amount0.to_signed_bytes_be(),
        amount1_raw: amount1.to_signed_bytes_be(),
    })
}

fn token_amount_computation(
    event: &Event,
    modify_liquidity: Option<&ModifyLiquidityComputation>,
    token0_decimals: &Option<StoreBigInt>,
    token1_decimals: &Option<StoreBigInt>,
) -> Option<TokenAmountComputation> {
    let token0_decimals = math_bigint(token0_decimals.as_ref()?);
    let token1_decimals = math_bigint(token1_decimals.as_ref()?);
    let (amount0, amount1) = match event.payload.as_ref()? {
        event::Payload::ModifyLiquidity(_) => {
            let computation = modify_liquidity?;
            (
                MathBigInt::from_signed_bytes_be(&computation.amount0_raw),
                MathBigInt::from_signed_bytes_be(&computation.amount1_raw),
            )
        }
        event::Payload::Swap(value) => (
            value.amount0.parse::<MathBigInt>().ok()?,
            value.amount1.parse::<MathBigInt>().ok()?,
        ),
        _ => return None,
    };
    let mut amount0 = decimal::token_to_decimal(&amount0, &token0_decimals);
    let mut amount1 = decimal::token_to_decimal(&amount1, &token1_decimals);
    if matches!(event.payload.as_ref(), Some(event::Payload::Swap(_))) {
        let negative_one = decimal::GraphDecimal::from(-1);
        amount0 = amount0 * negative_one.clone();
        amount1 = amount1 * negative_one;
    }
    Some(TokenAmountComputation {
        amount0: amount0.to_string(),
        amount1: amount1.to_string(),
    })
}

fn pool_prices(
    event: &Event,
    sqrt_price_after: &Option<StoreBigInt>,
    token0_decimals: &Option<StoreBigInt>,
    token1_decimals: &Option<StoreBigInt>,
) -> Option<(String, String)> {
    if !matches!(
        event.payload.as_ref(),
        Some(event::Payload::Initialize(_) | event::Payload::Swap(_))
    ) {
        return None;
    }
    let sqrt_price = math_bigint(sqrt_price_after.as_ref()?);
    let token0_decimals = math_bigint(token0_decimals.as_ref()?);
    let token1_decimals = math_bigint(token1_decimals.as_ref()?);
    let (token0_price, token1_price) =
        decimal::sqrt_price_x96_to_token_prices(&sqrt_price, &token0_decimals, &token1_decimals);
    Some((token0_price.to_string(), token1_price.to_string()))
}

fn math_bigint(value: &StoreBigInt) -> MathBigInt {
    MathBigInt::from_signed_bytes_be(&value.to_signed_bytes_be())
}

fn reducer_event(event: &Event, tick_values: &TickDeltaValues) -> ReducerEvent {
    let ord = ordinal(event);
    let ticks = event_ticks(event)
        .into_iter()
        .map(|(_, id)| {
            let gross = tick_values
                .get(&(ord, format!("gross:{id}")))
                .cloned()
                .unwrap_or_else(|| panic!("missing gross Tick Store delta for {id} at {ord}"));
            let net = tick_values
                .get(&(ord, format!("net:{id}")))
                .cloned()
                .unwrap_or_else(|| panic!("missing net Tick Store delta for {id} at {ord}"));
            TickPrimitiveState {
                liquidity_gross_before: gross.0,
                liquidity_gross_after: gross.1,
                liquidity_net_before: net.0,
                liquidity_net_after: net.1,
            }
        })
        .collect();
    ReducerEvent {
        event: Some(event.clone()),
        pool: None,
        ticks,
        modify_liquidity: None,
        token_amounts: None,
    }
}

fn bigint_hex(value: &str) -> MathBigInt {
    MathBigInt::parse_bytes(value.trim_start_matches("0x").as_bytes(), 16)
        .expect("pinned hexadecimal constant is valid")
}

fn mul_shift(value: MathBigInt, multiplier: &str) -> MathBigInt {
    (value * bigint_hex(multiplier)) >> 128
}

fn sqrt_ratio_at_tick(tick: i32) -> MathBigInt {
    assert!((-887_272..=887_272).contains(&tick), "TICK");
    let abs_tick = tick.unsigned_abs();
    let mut ratio = if abs_tick & 0x1 != 0 {
        bigint_hex("fffcb933bd6fad37aa2d162d1a594001")
    } else {
        bigint_hex("100000000000000000000000000000000")
    };
    for (mask, multiplier) in [
        (0x2, "fff97272373d413259a46990580e213a"),
        (0x4, "fff2e50f5f656932ef12357cf3c7fdcc"),
        (0x8, "ffe5caca7e10e4e61c3624eaa0941cd0"),
        (0x10, "ffcb9843d60f6159c9db58835c926644"),
        (0x20, "ff973b41fa98c081472e6896dfb254c0"),
        (0x40, "ff2ea16466c96a3843ec78b326b52861"),
        (0x80, "fe5dee046a99a2a811c461f1969c3053"),
        (0x100, "fcbe86c7900a88aedcffc83b479aa3a4"),
        (0x200, "f987a7253ac413176f2b074cf7815e54"),
        (0x400, "f3392b0822b70005940c7a398e4b70f3"),
        (0x800, "e7159475a2c29b7443b29c7fa6e889d9"),
        (0x1000, "d097f3bdfd2022b8845ad8f792aa5825"),
        (0x2000, "a9f746462d870fdf8a65dc1f90e061e5"),
        (0x4000, "70d869a156d2a1b890bb3df62baf32f7"),
        (0x8000, "31be135f97d08fd981231505542fcfa6"),
        (0x10000, "9aa508b5b7a84e1c677de54f3e99bc9"),
        (0x20000, "5d6af8dedb81196699c329225ee604"),
        (0x40000, "2216e584f5fa1ea926041bedfe98"),
        (0x80000, "48a170391f7dc42444e8fa2"),
    ] {
        if abs_tick & mask != 0 {
            ratio = mul_shift(ratio, multiplier);
        }
    }
    if tick > 0 {
        ratio = ((MathBigInt::one() << 256) - MathBigInt::one()) / ratio;
    }
    let denominator: MathBigInt = MathBigInt::one() << 32;
    let remainder: MathBigInt = &ratio % &denominator;
    ratio / denominator
        + if remainder.is_zero() {
            MathBigInt::zero()
        } else {
            MathBigInt::one()
        }
}

fn mul_div_rounding_up(a: &MathBigInt, b: &MathBigInt, denominator: &MathBigInt) -> MathBigInt {
    let product = a * b;
    let result = &product / denominator;
    result
        + if (&product % denominator).is_zero() {
            MathBigInt::zero()
        } else {
            MathBigInt::one()
        }
}

fn amount0_delta(
    a: MathBigInt,
    b: MathBigInt,
    liquidity: &MathBigInt,
    round_up: bool,
) -> MathBigInt {
    let (a, b) = if a > b { (b, a) } else { (a, b) };
    let numerator1 = liquidity << 96;
    let numerator2 = &b - &a;
    if round_up {
        mul_div_rounding_up(
            &mul_div_rounding_up(&numerator1, &numerator2, &b),
            &MathBigInt::one(),
            &a,
        )
    } else {
        numerator1 * numerator2 / b / a
    }
}

fn amount1_delta(
    a: MathBigInt,
    b: MathBigInt,
    liquidity: &MathBigInt,
    round_up: bool,
) -> MathBigInt {
    let (a, b) = if a > b { (b, a) } else { (a, b) };
    let difference = b - a;
    let q96 = MathBigInt::one() << 96;
    if round_up {
        mul_div_rounding_up(liquidity, &difference, &q96)
    } else {
        liquidity * difference / q96
    }
}

fn liquidity_amounts(
    tick_lower: i32,
    tick_upper: i32,
    current_tick: i32,
    liquidity: &MathBigInt,
    current_sqrt_price_x96: &MathBigInt,
) -> (MathBigInt, MathBigInt) {
    let a = sqrt_ratio_at_tick(tick_lower);
    let b = sqrt_ratio_at_tick(tick_upper);
    let round_up = liquidity.is_positive();
    let amount0 = if current_tick < tick_lower {
        amount0_delta(a.clone(), b.clone(), liquidity, round_up)
    } else if current_tick < tick_upper {
        amount0_delta(
            current_sqrt_price_x96.clone(),
            b.clone(),
            liquidity,
            round_up,
        )
    } else {
        MathBigInt::zero()
    };
    let amount1 = if current_tick < tick_lower {
        MathBigInt::zero()
    } else if current_tick < tick_upper {
        amount1_delta(
            a.clone(),
            current_sqrt_price_x96.clone(),
            liquidity,
            round_up,
        )
    } else {
        amount1_delta(a, b, liquidity, round_up)
    };
    (amount0, amount1)
}

fn event_ticks(event: &Event) -> Vec<(i32, String)> {
    let Some(event::Payload::ModifyLiquidity(value)) = event.payload.as_ref() else {
        return Vec::new();
    };
    let pool_id = hex(&value.pool_id);
    vec![
        (
            value
                .tick_lower
                .parse()
                .expect("decoded lower Tick is an i32"),
            format!("{}#{}", pool_id, value.tick_lower),
        ),
        (
            value
                .tick_upper
                .parse()
                .expect("decoded upper Tick is an i32"),
            format!("{}#{}", pool_id, value.tick_upper),
        ),
    ]
}

fn event_pool(event: &Event) -> Option<String> {
    match event.payload.as_ref()? {
        event::Payload::Initialize(value) => Some(hex(&value.pool_id)),
        event::Payload::ModifyLiquidity(value) => Some(hex(&value.pool_id)),
        event::Payload::Swap(value) => Some(hex(&value.pool_id)),
        _ => None,
    }
}

fn ordinal(event: &Event) -> u64 {
    event.log.as_ref().map_or(0, |value| value.ordinal)
}

fn hex(value: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut output = String::with_capacity(2 + value.len() * 2);
    output.push_str("0x");
    for byte in value {
        write!(&mut output, "{byte:02x}").expect("writing to a String is infallible");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use pb::events::{Initialize, LogRef};

    #[test]
    fn selects_only_pool_events() {
        let event = Event {
            log: Some(LogRef {
                ordinal: 99,
                ..Default::default()
            }),
            payload: Some(event::Payload::Initialize(Initialize {
                pool_id: vec![0x11; 32],
                ..Default::default()
            })),
            ..Default::default()
        };

        assert_eq!(ordinal(&event), 99);
        assert_eq!(event_pool(&event), Some(format!("0x{}", "11".repeat(32))));
    }

    #[test]
    fn selects_modify_liquidity_tick_ids_in_mapping_order() {
        let event = Event {
            payload: Some(event::Payload::ModifyLiquidity(
                pb::events::ModifyLiquidity {
                    pool_id: vec![0x22; 32],
                    tick_lower: "-60".to_owned(),
                    tick_upper: "60".to_owned(),
                    ..Default::default()
                },
            )),
            ..Default::default()
        };

        assert_eq!(
            event_ticks(&event),
            vec![
                (-60, format!("0x{}#-60", "22".repeat(32))),
                (60, format!("0x{}#60", "22".repeat(32))),
            ]
        );
    }
}
