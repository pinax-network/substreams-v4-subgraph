#![allow(clippy::not_unsafe_ptr_arg_deref)]

use substreams::store::{DeltaBigInt, Deltas, StoreGet, StoreGetBigInt};

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
    }
}

use pb::{
    events::{event, Event, Events},
    probe::{PoolState, StoreProbe, TickDelta},
};

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
}
