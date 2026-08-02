use crate::{
    abi::{arrakis_hook_factory, pool_manager, position_manager},
    config::{Config, ARRAKIS_HOOK_FACTORY_START, POOL_MANAGER_START, POSITION_MANAGER_START},
    pb::pinax::uniswap::v4::base::v1 as pb,
};
use substreams_ethereum::{pb::eth::v2, Event as _};

pub fn map_block(config: &Config, block: &v2::Block) -> pb::Events {
    let mut events = Vec::new();

    for transaction in block.transactions() {
        let Some(receipt) = transaction.receipt.as_ref() else {
            continue;
        };

        for log in &receipt.logs {
            let payload =
                if block.number >= POOL_MANAGER_START && log.address == config.pool_manager {
                    decode_pool_manager(log).map(|payload| (pb::DataSource::PoolManager, payload))
                } else if block.number >= POSITION_MANAGER_START
                    && log.address == config.position_manager
                {
                    decode_position_manager(log)
                        .map(|payload| (pb::DataSource::PositionManager, payload))
                } else if block.number >= ARRAKIS_HOOK_FACTORY_START
                    && log.address == config.arrakis_hook_factory
                {
                    decode_arrakis_hook_factory(log)
                        .map(|payload| (pb::DataSource::ArrakisHookFactory, payload))
                } else {
                    None
                };

            if let Some((source, payload)) = payload {
                events.push(event_metadata(block, transaction, log, source, payload));
            }
        }
    }

    // Graph Node v0.44 orders Ethereum event triggers by the block-global log index.
    // Sorting explicitly keeps this contract true even if an upstream block producer
    // presents receipts or logs in a non-canonical vector order.
    events.sort_by_key(|event| {
        let log = event
            .log
            .as_ref()
            .expect("decoded events always have log metadata");
        let transaction = event
            .transaction
            .as_ref()
            .expect("decoded events always have transaction metadata");
        (
            log.graph_node_trigger_order,
            transaction.index,
            log.transaction_log_index,
        )
    });

    pb::Events { events }
}

fn decode_pool_manager(log: &v2::Log) -> Option<pb::event::Payload> {
    if let Some(event) = pool_manager::events::Initialize::match_and_decode(log) {
        return Some(pb::event::Payload::Initialize(pb::Initialize {
            pool_id: event.id.to_vec(),
            currency0: event.currency0,
            currency1: event.currency1,
            fee: event.fee.to_string(),
            tick_spacing: event.tick_spacing.to_string(),
            hooks: event.hooks,
            sqrt_price_x96: event.sqrt_price_x96.to_string(),
            tick: event.tick.to_string(),
            token0_metadata: None,
            token1_metadata: None,
        }));
    }

    if let Some(event) = pool_manager::events::ModifyLiquidity::match_and_decode(log) {
        return Some(pb::event::Payload::ModifyLiquidity(pb::ModifyLiquidity {
            pool_id: event.id.to_vec(),
            sender: event.sender,
            tick_lower: event.tick_lower.to_string(),
            tick_upper: event.tick_upper.to_string(),
            liquidity_delta: event.liquidity_delta.to_string(),
            salt: event.salt.to_vec(),
        }));
    }

    pool_manager::events::Swap::match_and_decode(log).map(|event| {
        pb::event::Payload::Swap(pb::Swap {
            pool_id: event.id.to_vec(),
            sender: event.sender,
            amount0: event.amount0.to_string(),
            amount1: event.amount1.to_string(),
            sqrt_price_x96: event.sqrt_price_x96.to_string(),
            liquidity: event.liquidity.to_string(),
            tick: event.tick.to_string(),
            fee: event.fee.to_string(),
        })
    })
}

fn decode_position_manager(log: &v2::Log) -> Option<pb::event::Payload> {
    if let Some(event) = position_manager::events::Subscription::match_and_decode(log) {
        return Some(pb::event::Payload::Subscription(pb::Subscription {
            token_id: event.token_id.to_string(),
            subscriber: event.subscriber,
        }));
    }

    if let Some(event) = position_manager::events::Unsubscription::match_and_decode(log) {
        return Some(pb::event::Payload::Unsubscription(pb::Unsubscription {
            token_id: event.token_id.to_string(),
            subscriber: event.subscriber,
        }));
    }

    position_manager::events::Transfer::match_and_decode(log).map(|event| {
        pb::event::Payload::Transfer(pb::Transfer {
            from: event.from,
            to: event.to,
            token_id: event.id.to_string(),
        })
    })
}

fn decode_arrakis_hook_factory(log: &v2::Log) -> Option<pb::event::Payload> {
    arrakis_hook_factory::events::LogCreatePrivateHook::match_and_decode(log).map(|event| {
        pb::event::Payload::LogCreatePrivateHook(pb::LogCreatePrivateHook {
            hook: event.hook,
            module: event.module,
            salt: event.salt.to_vec(),
        })
    })
}

fn event_metadata(
    block: &v2::Block,
    transaction: &v2::TransactionTrace,
    log: &v2::Log,
    source: pb::DataSource,
    payload: pb::event::Payload,
) -> pb::Event {
    let header = block.header.as_ref();
    let timestamp = header.and_then(|header| header.timestamp.as_ref());

    pb::Event {
        block: Some(pb::BlockRef {
            number: block.number,
            hash: block.hash.clone(),
            parent_hash: header
                .map(|header| header.parent_hash.clone())
                .unwrap_or_default(),
            timestamp_seconds: timestamp.map(|value| value.seconds).unwrap_or_default(),
            timestamp_nanos: timestamp.map(|value| value.nanos).unwrap_or_default(),
        }),
        transaction: Some(pb::TransactionRef {
            index: transaction.index,
            hash: transaction.hash.clone(),
            origin: transaction.from.clone(),
            to: transaction.to.clone(),
            gas_price: transaction
                .gas_price
                .as_ref()
                .map(|value| {
                    let value: substreams::scalar::BigInt = value.into();
                    value.to_string()
                })
                .unwrap_or_else(|| "0".to_owned()),
        }),
        log: Some(pb::LogRef {
            transaction_log_index: log.index,
            block_log_index: log.block_index,
            ordinal: log.ordinal,
            address: log.address.clone(),
            graph_node_trigger_order: u64::from(log.block_index),
        }),
        source: source as i32,
        payload: Some(payload),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PINNED_PARAMS;
    use ethabi::{Address, Int, Token, Uint};
    use prost_types::Timestamp;
    use serde::Deserialize;
    use std::collections::BTreeMap;
    use substreams::Hex;

    #[derive(Deserialize)]
    struct Fixture {
        blocks: Vec<FixtureBlock>,
    }

    #[derive(Deserialize)]
    struct FixtureBlock {
        number: u64,
        hash: String,
        parent_hash: String,
        timestamp_seconds: i64,
        transactions: Vec<FixtureTransaction>,
    }

    #[derive(Deserialize)]
    struct FixtureTransaction {
        index: u32,
        hash: String,
        from: String,
        to: String,
        status: i32,
        logs: Vec<FixtureLog>,
    }

    #[derive(Deserialize)]
    struct FixtureLog {
        event: String,
        address: String,
        topics: Vec<String>,
        data: String,
        transaction_log_index: u32,
        block_log_index: u32,
    }

    #[test]
    fn decodes_canonical_receipt_fixtures_offline() {
        let fixture: Fixture =
            serde_json::from_str(include_str!("../fixtures/event-logs.json")).unwrap();
        let config = Config::parse(PINNED_PARAMS).unwrap();
        let mut counts = BTreeMap::new();

        for fixture_block in fixture.blocks {
            let expected = fixture_block
                .transactions
                .iter()
                .flat_map(|transaction| transaction.logs.iter())
                .map(|log| (log.block_log_index, log.event.clone()))
                .collect::<Vec<_>>();
            let block = fixture_block.into_block();
            let decoded = map_block(&config, &block);
            let actual = decoded
                .events
                .iter()
                .map(|event| {
                    let log = event.log.as_ref().unwrap();
                    let name = payload_name(event.payload.as_ref().unwrap());
                    *counts.entry(name).or_insert(0usize) += 1;
                    (log.block_log_index, name.to_owned())
                })
                .collect::<Vec<_>>();

            let mut expected = expected;
            expected.sort_by_key(|(block_log_index, _)| *block_log_index);
            assert_eq!(actual, expected);
            assert!(decoded.events.windows(2).all(|pair| {
                pair[0].log.as_ref().unwrap().graph_node_trigger_order
                    < pair[1].log.as_ref().unwrap().graph_node_trigger_order
            }));
            for event in decoded.events {
                assert_eq!(event.block.as_ref().unwrap().number, block.number);
                assert_eq!(
                    event.log.as_ref().unwrap().graph_node_trigger_order,
                    u64::from(event.log.as_ref().unwrap().block_log_index)
                );
                assert_eq!(event.transaction.as_ref().unwrap().origin.len(), 20);
            }
        }

        for name in [
            "Initialize",
            "ModifyLiquidity",
            "Swap",
            "Subscription",
            "Unsubscription",
            "Transfer",
            "LogCreatePrivateHook",
        ] {
            assert!(counts.get(name).copied().unwrap_or_default() > 0, "{name}");
        }
    }

    #[test]
    fn ignores_malformed_and_wrong_address_logs() {
        let config = Config::parse(PINNED_PARAMS).unwrap();
        let mut log = swap_log(1, 2, -1, 1);
        log.data.truncate(31);
        let malformed = block_with_logs(POOL_MANAGER_START, vec![log]);
        assert!(map_block(&config, &malformed).events.is_empty());

        let mut log = swap_log(1, 2, -1, 1);
        log.address = vec![0; 20];
        let wrong_address = block_with_logs(POOL_MANAGER_START, vec![log]);
        assert!(map_block(&config, &wrong_address).events.is_empty());
    }

    #[test]
    fn preserves_signed_boundaries_and_graph_node_order() {
        let config = Config::parse(PINNED_PARAMS).unwrap();
        let initialize = initialize_log(4, -8_388_608, 8_388_607);
        let swap = swap_log(9, 1, i128::MIN, i128::MAX);
        let decoded = map_block(
            &config,
            &block_with_logs(POOL_MANAGER_START, vec![swap, initialize]),
        );

        assert_eq!(decoded.events.len(), 2);
        assert_eq!(decoded.events[0].log.as_ref().unwrap().block_log_index, 4);
        assert_eq!(decoded.events[1].log.as_ref().unwrap().block_log_index, 9);

        let pb::event::Payload::Initialize(event) = decoded.events[0].payload.as_ref().unwrap()
        else {
            panic!("expected Initialize")
        };
        assert_eq!(event.tick_spacing, "-8388608");
        assert_eq!(event.tick, "8388607");

        let pb::event::Payload::Swap(event) = decoded.events[1].payload.as_ref().unwrap() else {
            panic!("expected Swap")
        };
        assert_eq!(event.amount0, i128::MIN.to_string());
        assert_eq!(event.amount1, i128::MAX.to_string());
    }

    fn payload_name(payload: &pb::event::Payload) -> &'static str {
        match payload {
            pb::event::Payload::Initialize(_) => "Initialize",
            pb::event::Payload::ModifyLiquidity(_) => "ModifyLiquidity",
            pb::event::Payload::Swap(_) => "Swap",
            pb::event::Payload::Subscription(_) => "Subscription",
            pb::event::Payload::Unsubscription(_) => "Unsubscription",
            pb::event::Payload::Transfer(_) => "Transfer",
            pb::event::Payload::LogCreatePrivateHook(_) => "LogCreatePrivateHook",
        }
    }

    impl FixtureBlock {
        fn into_block(self) -> v2::Block {
            v2::Block {
                hash: decode_hex(&self.hash),
                number: self.number,
                header: Some(v2::BlockHeader {
                    parent_hash: decode_hex(&self.parent_hash),
                    number: self.number,
                    timestamp: Some(Timestamp {
                        seconds: self.timestamp_seconds,
                        nanos: 0,
                    }),
                    ..Default::default()
                }),
                transaction_traces: self
                    .transactions
                    .into_iter()
                    .map(FixtureTransaction::into_trace)
                    .collect(),
                ..Default::default()
            }
        }
    }

    impl FixtureTransaction {
        fn into_trace(self) -> v2::TransactionTrace {
            v2::TransactionTrace {
                index: self.index,
                hash: decode_hex(&self.hash),
                from: decode_hex(&self.from),
                to: decode_hex(&self.to),
                status: self.status,
                receipt: Some(v2::TransactionReceipt {
                    logs: self.logs.into_iter().map(FixtureLog::into_log).collect(),
                    ..Default::default()
                }),
                ..Default::default()
            }
        }
    }

    impl FixtureLog {
        fn into_log(self) -> v2::Log {
            v2::Log {
                address: decode_hex(&self.address),
                topics: self.topics.iter().map(|topic| decode_hex(topic)).collect(),
                data: decode_hex(&self.data),
                index: self.transaction_log_index,
                block_index: self.block_log_index,
                ordinal: u64::from(self.block_log_index) * 2 + 1,
            }
        }
    }

    fn block_with_logs(number: u64, logs: Vec<v2::Log>) -> v2::Block {
        v2::Block {
            number,
            hash: vec![0x11; 32],
            header: Some(v2::BlockHeader {
                parent_hash: vec![0x22; 32],
                number,
                timestamp: Some(Timestamp {
                    seconds: 1_700_000_000,
                    nanos: 0,
                }),
                ..Default::default()
            }),
            transaction_traces: vec![v2::TransactionTrace {
                index: 7,
                hash: vec![0x33; 32],
                from: vec![0x44; 20],
                to: vec![0x55; 20],
                status: 1,
                receipt: Some(v2::TransactionReceipt {
                    logs,
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn initialize_log(block_index: u32, tick_spacing: i32, tick: i32) -> v2::Log {
        v2::Log {
            address: decode_hex(crate::config::POOL_MANAGER),
            topics: vec![
                decode_hex("dd466e674ea557f56295e2d0218a125ea4b4f0f6f3307b95f85e6110838d6438"),
                vec![0xaa; 32],
                address_topic(0x10),
                address_topic(0x20),
            ],
            data: ethabi::encode(&[
                Token::Uint(Uint::from(3_000u64)),
                Token::Int(signed_int(i128::from(tick_spacing))),
                Token::Address(Address::from_slice(&[0x30; 20])),
                Token::Uint(Uint::from(1u64) << 96),
                Token::Int(signed_int(i128::from(tick))),
            ]),
            index: block_index,
            block_index,
            ordinal: u64::from(block_index) * 2 + 1,
        }
    }

    fn swap_log(block_index: u32, transaction_index: u32, amount0: i128, amount1: i128) -> v2::Log {
        v2::Log {
            address: decode_hex(crate::config::POOL_MANAGER),
            topics: vec![
                decode_hex("40e9cecb9f5f1f1c5b9c97dec2917b7ee92e57ba5563708daca94dd84ad7112f"),
                vec![0xbb; 32],
                address_topic(0x40),
            ],
            data: ethabi::encode(&[
                Token::Int(signed_int(amount0)),
                Token::Int(signed_int(amount1)),
                Token::Uint(Uint::from(1u64) << 96),
                Token::Uint(Uint::from(1_000_000u64)),
                Token::Int(signed_int(-1)),
                Token::Uint(Uint::from(500u64)),
            ]),
            index: transaction_index,
            block_index,
            ordinal: u64::from(block_index) * 2 + 1,
        }
    }

    fn signed_int(value: i128) -> Int {
        let mut bytes = [if value.is_negative() { 0xff } else { 0x00 }; 32];
        bytes[16..].copy_from_slice(&value.to_be_bytes());
        Int::from_big_endian(&bytes)
    }

    fn address_topic(byte: u8) -> Vec<u8> {
        let mut topic = vec![0; 12];
        topic.extend([byte; 20]);
        topic
    }

    fn decode_hex(value: &str) -> Vec<u8> {
        let value = value.strip_prefix("0x").unwrap_or(value);
        if value.is_empty() {
            return Vec::new();
        }
        Hex::decode(value).unwrap()
    }
}
