#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::str::FromStr;

use substreams::{
    scalar::BigInt,
    store::{StoreNew, StoreSet, StoreSetBigInt},
};

mod pb {
    pub mod events {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src/pb/pinax.uniswap.v4.base.v1.rs"
        ));
    }
}

use pb::events::{event, Events};

/// Absolute Pool sqrt price. Unlike `add` state, every value comes directly
/// from Initialize or Swap and is therefore safe under last-value-wins merge.
#[substreams::handlers::store]
pub fn store_pool_sqrt_price(events: Events, output: StoreSetBigInt) {
    for event in &events.events {
        let Some((pool_id, sqrt_price)) =
            event.payload.as_ref().and_then(|payload| match payload {
                event::Payload::Initialize(value) => Some((&value.pool_id, &value.sqrt_price_x96)),
                event::Payload::Swap(value) => Some((&value.pool_id, &value.sqrt_price_x96)),
                _ => None,
            })
        else {
            continue;
        };
        let Ok(sqrt_price) = BigInt::from_str(sqrt_price) else {
            continue;
        };
        output.set(ordinal(event.log.as_ref()), hex(pool_id), &sqrt_price);
    }
}

fn ordinal(log: Option<&pb::events::LogRef>) -> u64 {
    log.map_or(0, |value| value.ordinal)
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
    use pb::events::{Event, Initialize, LogRef};

    #[test]
    fn extracts_initialize_sqrt_price() {
        let event = Event {
            log: Some(LogRef {
                ordinal: 99,
                ..Default::default()
            }),
            payload: Some(event::Payload::Initialize(Initialize {
                pool_id: vec![0x11; 32],
                sqrt_price_x96: "79228162514264337593543950336".to_owned(),
                ..Default::default()
            })),
            ..Default::default()
        };
        let event::Payload::Initialize(value) = event.payload.as_ref().unwrap() else {
            unreachable!()
        };
        assert_eq!(ordinal(event.log.as_ref()), 99);
        assert_eq!(hex(&value.pool_id), format!("0x{}", "11".repeat(32)));
    }
}
