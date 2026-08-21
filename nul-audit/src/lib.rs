#![allow(clippy::not_unsafe_ptr_arg_deref)]

mod pb {
    #[allow(dead_code)]
    pub mod events {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src/pb/pinax.uniswap.v4.base.v1.rs"
        ));
    }

    pub mod reducer {
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
        }
    }

    pub mod audit {
        #[derive(Clone, PartialEq, prost::Message)]
        pub struct NulMetadataAudit {
            #[prost(message, repeated, tag = "1")]
            pub occurrences: Vec<NulMetadataOccurrence>,
        }

        #[derive(Clone, PartialEq, prost::Message)]
        pub struct NulMetadataOccurrence {
            #[prost(uint64, tag = "1")]
            pub block_number: u64,
            #[prost(bytes = "vec", tag = "2")]
            pub block_hash: Vec<u8>,
            #[prost(bytes = "vec", tag = "3")]
            pub transaction_hash: Vec<u8>,
            #[prost(uint64, tag = "4")]
            pub log_ordinal: u64,
            #[prost(bytes = "vec", tag = "5")]
            pub token_address: Vec<u8>,
            #[prost(string, tag = "6")]
            pub field: String,
            #[prost(bytes = "vec", tag = "7")]
            pub raw_utf8: Vec<u8>,
            #[prost(string, tag = "8")]
            pub corrected_value: String,
        }
    }
}

use pb::{
    audit::{NulMetadataAudit, NulMetadataOccurrence},
    events::{event, Event, TokenMetadata},
    reducer::ReducerInputs,
};

#[substreams::handlers::map]
pub fn map_nul_metadata_audit(inputs: ReducerInputs) -> NulMetadataAudit {
    audit_inputs(inputs)
}

fn audit_inputs(inputs: ReducerInputs) -> NulMetadataAudit {
    let mut occurrences = Vec::new();
    for reducer_event in inputs.events {
        let Some(event) = reducer_event.event.as_ref() else {
            continue;
        };
        let Some(event::Payload::Initialize(initialize)) = event.payload.as_ref() else {
            continue;
        };
        append_metadata(
            &mut occurrences,
            event,
            &initialize.currency0,
            initialize.token0_metadata.as_ref(),
        );
        append_metadata(
            &mut occurrences,
            event,
            &initialize.currency1,
            initialize.token1_metadata.as_ref(),
        );
    }
    NulMetadataAudit { occurrences }
}

fn append_metadata(
    output: &mut Vec<NulMetadataOccurrence>,
    event: &Event,
    token_address: &[u8],
    metadata: Option<&TokenMetadata>,
) {
    let Some(metadata) = metadata else {
        return;
    };
    append_field(output, event, token_address, "symbol", &metadata.symbol);
    append_field(output, event, token_address, "name", &metadata.name);
}

fn append_field(
    output: &mut Vec<NulMetadataOccurrence>,
    event: &Event,
    token_address: &[u8],
    field: &str,
    value: &str,
) {
    let Some(nul) = value.as_bytes().iter().position(|byte| *byte == 0) else {
        return;
    };
    let block = event.block.as_ref();
    let transaction = event.transaction.as_ref();
    let log = event.log.as_ref();
    output.push(NulMetadataOccurrence {
        block_number: block.map_or(0, |value| value.number),
        block_hash: block.map_or_else(Vec::new, |value| value.hash.clone()),
        transaction_hash: transaction.map_or_else(Vec::new, |value| value.hash.clone()),
        log_ordinal: log.map_or(0, |value| value.ordinal),
        token_address: token_address.to_vec(),
        field: field.to_owned(),
        raw_utf8: value.as_bytes().to_vec(),
        corrected_value: value[..nul].to_owned(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use pb::events::{BlockRef, Initialize, LogRef, TransactionRef};

    fn reducer_event(symbol: &str, name: &str) -> pb::reducer::ReducerEvent {
        pb::reducer::ReducerEvent {
            event: Some(Event {
                block: Some(BlockRef {
                    number: 34_546_213,
                    hash: vec![0x11; 32],
                    ..Default::default()
                }),
                transaction: Some(TransactionRef {
                    hash: vec![0x22; 32],
                    ..Default::default()
                }),
                log: Some(LogRef {
                    ordinal: 99,
                    ..Default::default()
                }),
                payload: Some(event::Payload::Initialize(Initialize {
                    currency0: vec![0x33; 20],
                    currency1: vec![0x44; 20],
                    token0_metadata: Some(TokenMetadata {
                        symbol: symbol.to_owned(),
                        name: name.to_owned(),
                        ..Default::default()
                    }),
                    token1_metadata: Some(TokenMetadata {
                        symbol: "clean".to_owned(),
                        name: "clean".to_owned(),
                        ..Default::default()
                    }),
                    ..Default::default()
                })),
                ..Default::default()
            }),
        }
    }

    #[test]
    fn emits_only_nul_bearing_metadata_fields() {
        let audit = audit_inputs(ReducerInputs {
            events: vec![reducer_event("ZT-0000\0\0", "ZT-0000\0\0")],
            state_version: 3,
        });
        assert_eq!(audit.occurrences.len(), 2);
        assert_eq!(audit.occurrences[0].field, "symbol");
        assert_eq!(audit.occurrences[0].corrected_value, "ZT-0000");
        assert_eq!(audit.occurrences[0].token_address, vec![0x33; 20]);
        assert_eq!(audit.occurrences[1].field, "name");
    }

    #[test]
    fn empty_output_has_no_encoded_payload() {
        use prost::Message as _;

        let audit = audit_inputs(ReducerInputs {
            events: vec![reducer_event("clean", "clean")],
            state_version: 3,
        });
        assert!(audit.occurrences.is_empty());
        assert_eq!(audit.encoded_len(), 0);
    }
}
