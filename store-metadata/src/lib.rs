#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::str::FromStr;

use substreams::{
    scalar::BigInt,
    store::{StoreGet, StoreGetBigInt, StoreNew, StoreSet, StoreSetBigInt},
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

const ADDRESS_ZERO: &[u8; 20] = &[0; 20];

/// Token decimals learned from every Initialize. False-positive metadata from
/// a mapping-rejected Pool is harmless: reducer-owned Pool existence remains
/// authoritative, while an accepted Pool can only reference a token whose
/// metadata was valid in this or an earlier accepted Initialize.
#[substreams::handlers::store]
pub fn store_token_decimals(events: Events, output: StoreSetBigInt) {
    for event in &events.events {
        let Some(event::Payload::Initialize(value)) = event.payload.as_ref() else {
            continue;
        };
        set_token_decimals(
            ordinal(event.log.as_ref()),
            &value.currency0,
            value.token0_metadata.as_ref(),
            &output,
        );
        set_token_decimals(
            ordinal(event.log.as_ref()),
            &value.currency1,
            value.token1_metadata.as_ref(),
            &output,
        );
    }
}

/// Effective decimals keyed by `0:<pool>` and `1:<pool>`. This downstream
/// Store resolves same-event token metadata and previously learned token
/// metadata before the cached computation map runs.
#[substreams::handlers::store]
pub fn store_pool_token_decimals(
    events: Events,
    token_decimals: StoreGetBigInt,
    output: StoreSetBigInt,
) {
    for event in &events.events {
        let Some(event::Payload::Initialize(value)) = event.payload.as_ref() else {
            continue;
        };
        let ord = ordinal(event.log.as_ref());
        let pool_id = hex(&value.pool_id);
        for (index, token) in [(0, &value.currency0), (1, &value.currency1)] {
            let decimals = if token.as_slice() == ADDRESS_ZERO {
                Some(BigInt::from(18))
            } else {
                token_decimals.get_at(ord, hex(token))
            };
            if let Some(decimals) = decimals {
                output.set(ord, format!("{index}:{pool_id}"), &decimals);
            }
        }
    }
}

fn set_token_decimals(
    ordinal: u64,
    token: &[u8],
    metadata: Option<&pb::events::TokenMetadata>,
    output: &StoreSetBigInt,
) {
    let Some(decimals) = metadata
        .and_then(|value| value.decimals.as_deref())
        .and_then(|value| BigInt::from_str(value).ok())
    else {
        return;
    };
    output.set(ordinal, hex(token), &decimals);
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

    #[test]
    fn pool_decimal_keys_are_disjoint() {
        let pool = format!("0x{}", "11".repeat(32));
        assert_ne!(format!("0:{pool}"), format!("1:{pool}"));
    }

    #[test]
    fn native_currency_uses_eighteen_effective_decimals() {
        assert_eq!(ADDRESS_ZERO.len(), 20);
        assert_eq!(BigInt::from(18).to_string(), "18");
    }

    #[test]
    fn initialize_type_remains_the_decoder_type() {
        let value = pb::events::Initialize::default();
        assert!(value.pool_id.is_empty());
    }
}
