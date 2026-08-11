#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::str::FromStr;

use substreams::{
    scalar::BigInt,
    store::{StoreAdd, StoreAddBigInt, StoreNew},
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

const TICK_STORE_SHARDS: u8 = 16;

macro_rules! tick_store_handler {
    ($handler:ident, $shard:expr) => {
        #[substreams::handlers::store]
        pub fn $handler(events: Events, output: StoreAddBigInt) {
            store_tick_liquidity_shard(events, output, $shard);
        }
    };
}

tick_store_handler!(store_tick_liquidity_00, 0);
tick_store_handler!(store_tick_liquidity_01, 1);
tick_store_handler!(store_tick_liquidity_02, 2);
tick_store_handler!(store_tick_liquidity_03, 3);
tick_store_handler!(store_tick_liquidity_04, 4);
tick_store_handler!(store_tick_liquidity_05, 5);
tick_store_handler!(store_tick_liquidity_06, 6);
tick_store_handler!(store_tick_liquidity_07, 7);
tick_store_handler!(store_tick_liquidity_08, 8);
tick_store_handler!(store_tick_liquidity_09, 9);
tick_store_handler!(store_tick_liquidity_10, 10);
tick_store_handler!(store_tick_liquidity_11, 11);
tick_store_handler!(store_tick_liquidity_12, 12);
tick_store_handler!(store_tick_liquidity_13, 13);
tick_store_handler!(store_tick_liquidity_14, 14);
tick_store_handler!(store_tick_liquidity_15, 15);

fn store_tick_liquidity_shard(events: Events, output: StoreAddBigInt, shard: u8) {
    for event in &events.events {
        let Some(event::Payload::ModifyLiquidity(value)) = event.payload.as_ref() else {
            continue;
        };
        if tick_store_shard(&value.pool_id) != shard {
            continue;
        }
        let Ok(delta) = BigInt::from_str(&value.liquidity_delta) else {
            continue;
        };
        let pool_id = hex(&value.pool_id);
        let lower = format!("{pool_id}#{}", value.tick_lower);
        let upper = format!("{pool_id}#{}", value.tick_upper);
        let ordinal = event.log.as_ref().map_or(0, |log| log.ordinal);
        output.add(ordinal, format!("gross:{lower}"), delta.clone());
        output.add(ordinal, format!("net:{lower}"), delta.clone());
        output.add(ordinal, format!("gross:{upper}"), delta.clone());
        output.add(ordinal, format!("net:{upper}"), -delta);
    }
}

fn tick_store_shard(pool_id: &[u8]) -> u8 {
    pool_id.first().copied().unwrap_or_default() % TICK_STORE_SHARDS
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

    #[test]
    fn partitions_every_first_byte_into_one_of_sixteen_shards() {
        let mut counts = [0_u8; TICK_STORE_SHARDS as usize];
        for first_byte in 0_u8..=u8::MAX {
            let shard = tick_store_shard(&[first_byte; 32]);
            assert!(shard < TICK_STORE_SHARDS);
            counts[shard as usize] += 1;
        }
        assert_eq!(counts, [16; TICK_STORE_SHARDS as usize]);
    }

    #[test]
    fn empty_pool_id_routes_deterministically() {
        assert_eq!(tick_store_shard(&[]), 0);
    }

    #[test]
    fn retains_the_legacy_tick_key_encoding() {
        let pool_id = vec![0x11; 32];
        let id = format!("{}#-60", hex(&pool_id));
        assert_eq!(
            format!("gross:{id}"),
            format!("gross:0x{}#-60", "11".repeat(32))
        );
        assert_eq!(
            format!("net:{id}"),
            format!("net:0x{}#-60", "11".repeat(32))
        );
    }
}
