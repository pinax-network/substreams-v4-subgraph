#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::collections::HashMap;

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

        #[derive(Clone, PartialEq, prost::Message)]
        pub struct ReducerInputs {
            #[prost(message, repeated, tag = "1")]
            pub events: Vec<ReducerEvent>,
        }

        #[derive(Clone, PartialEq, prost::Message)]
        pub struct ReducerEvent {
            #[prost(message, optional, tag = "1")]
            pub event: Option<super::events::Event>,
            #[prost(message, repeated, tag = "3")]
            pub ticks: Vec<TickPrimitiveState>,
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
    probe::{PoolState, ReducerEvent, ReducerInputs, StoreProbe, TickDelta, TickPrimitiveState},
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
    }
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
        ticks,
    }
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
