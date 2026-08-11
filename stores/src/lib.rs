#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::str::FromStr;

use substreams::{
    scalar::BigInt,
    store::{
        StoreAdd, StoreAddBigInt, StoreGet, StoreGetBigInt, StoreNew, StoreSet, StoreSetBigInt,
        StoreSetSum, StoreSetSumBigInt,
    },
};

mod pb {
    pub mod events {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src/pb/pinax.uniswap.v4.base.v1.rs"
        ));
    }
}

use pb::events::{event, Event, Events};

#[substreams::handlers::store]
pub fn store_pool_tick(events: Events, output: StoreSetBigInt) {
    for event in &events.events {
        let Some((pool_id, tick)) = pool_tick(event) else {
            continue;
        };
        output.set(ordinal(event), pool_id, &tick);
    }
}

#[substreams::handlers::store]
pub fn store_pool_transaction_count(events: Events, output: StoreAddBigInt) {
    for event in &events.events {
        let Some(pool_id) = changed_pool(event) else {
            continue;
        };
        output.add(ordinal(event), pool_id, BigInt::from(1));
    }
}

#[substreams::handlers::store]
pub fn store_tick_liquidity(events: Events, output: StoreAddBigInt) {
    for operation in events.events.iter().flat_map(tick_operations) {
        output.add(operation.ordinal, operation.key, operation.value);
    }
}

#[substreams::handlers::store]
pub fn store_pool_liquidity(events: Events, pool_ticks: StoreGetBigInt, output: StoreSetSumBigInt) {
    for event in &events.events {
        let ord = ordinal(event);
        match event.payload.as_ref() {
            Some(event::Payload::Initialize(value)) => {
                output.set(ord, hex(&value.pool_id), BigInt::from(0));
            }
            Some(event::Payload::Swap(value)) => {
                if let Some(liquidity) = bigint(&value.liquidity) {
                    output.set(ord, hex(&value.pool_id), liquidity);
                }
            }
            Some(event::Payload::ModifyLiquidity(value)) => {
                let pool_id = hex(&value.pool_id);
                let Some(current_tick) = pool_ticks
                    .get_at(ord, &pool_id)
                    .and_then(|value| i32::from_str(&value.to_string()).ok())
                else {
                    continue;
                };
                let (Ok(lower), Ok(upper), Some(delta)) = (
                    value.tick_lower.parse::<i32>(),
                    value.tick_upper.parse::<i32>(),
                    bigint(&value.liquidity_delta),
                ) else {
                    continue;
                };
                if lower <= current_tick && upper > current_tick {
                    output.sum(ord, pool_id, delta);
                }
            }
            _ => {}
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct TickOperation {
    ordinal: u64,
    key: String,
    value: BigInt,
}

fn tick_operations(event: &Event) -> Vec<TickOperation> {
    let Some(event::Payload::ModifyLiquidity(value)) = event.payload.as_ref() else {
        return Vec::new();
    };
    let Some(delta) = bigint(&value.liquidity_delta) else {
        return Vec::new();
    };
    let pool_id = hex(&value.pool_id);
    let lower = format!("{}#{}", pool_id, value.tick_lower);
    let upper = format!("{}#{}", pool_id, value.tick_upper);
    let ord = ordinal(event);
    vec![
        TickOperation {
            ordinal: ord,
            key: format!("gross:{lower}"),
            value: delta.clone(),
        },
        TickOperation {
            ordinal: ord,
            key: format!("net:{lower}"),
            value: delta.clone(),
        },
        TickOperation {
            ordinal: ord,
            key: format!("gross:{upper}"),
            value: delta.clone(),
        },
        TickOperation {
            ordinal: ord,
            key: format!("net:{upper}"),
            value: -delta,
        },
    ]
}

fn pool_tick(event: &Event) -> Option<(String, BigInt)> {
    match event.payload.as_ref()? {
        event::Payload::Initialize(value) => Some((hex(&value.pool_id), bigint(&value.tick)?)),
        event::Payload::Swap(value) => Some((hex(&value.pool_id), bigint(&value.tick)?)),
        _ => None,
    }
}

fn changed_pool(event: &Event) -> Option<String> {
    match event.payload.as_ref()? {
        event::Payload::ModifyLiquidity(value) => Some(hex(&value.pool_id)),
        event::Payload::Swap(value) => Some(hex(&value.pool_id)),
        _ => None,
    }
}

fn ordinal(event: &Event) -> u64 {
    event.log.as_ref().map_or(0, |value| value.ordinal)
}

fn bigint(value: &str) -> Option<BigInt> {
    BigInt::from_str(value).ok()
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
    use pb::events::{BlockRef, LogRef, ModifyLiquidity};

    #[test]
    fn tick_projection_preserves_mapping_order_and_signs() {
        let event = Event {
            block: Some(BlockRef {
                number: 42,
                ..Default::default()
            }),
            log: Some(LogRef {
                ordinal: 99,
                ..Default::default()
            }),
            payload: Some(event::Payload::ModifyLiquidity(ModifyLiquidity {
                pool_id: vec![0x11; 32],
                tick_lower: "-60".to_owned(),
                tick_upper: "60".to_owned(),
                liquidity_delta: "123".to_owned(),
                ..Default::default()
            })),
            ..Default::default()
        };

        let operations = tick_operations(&event);
        assert_eq!(operations.len(), 4);
        assert_eq!(operations[0].ordinal, 99);
        assert!(operations[0].key.starts_with("gross:0x1111"));
        assert!(operations[1].key.starts_with("net:0x1111"));
        assert_eq!(operations[0].value.to_string(), "123");
        assert_eq!(operations[2].value.to_string(), "123");
        assert_eq!(operations[3].value.to_string(), "-123");
    }
}
