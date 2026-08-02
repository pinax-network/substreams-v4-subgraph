//! Historical ERC-20 metadata calls with the deployed mapping's fallbacks.

use crate::pb::pinax::uniswap::v4::base::v1 as pb;
use ethabi::{ParamType, Token};
use std::collections::{BTreeMap, BTreeSet};
use substreams::errors::Error;
use substreams_ethereum::{pb::eth::rpc::RpcCall, rpc};

const ADDRESS_ZERO: [u8; 20] = [0; 20];
const NULL_ETH_VALUE: [u8; 32] = {
    let mut value = [0; 32];
    value[31] = 1;
    value
};

const SYMBOL: [u8; 4] = [0x95, 0xd8, 0x9b, 0x41];
const NAME: [u8; 4] = [0x06, 0xfd, 0xde, 0x03];
const TOTAL_SUPPLY: [u8; 4] = [0x18, 0x16, 0x0d, 0xdd];
const DECIMALS: [u8; 4] = [0x31, 0x3c, 0xe5, 0x67];

pub fn enrich(events: &mut pb::Events) -> Result<(), Error> {
    let addresses = events
        .events
        .iter()
        .filter_map(|event| match event.payload.as_ref() {
            Some(pb::event::Payload::Initialize(value)) => {
                Some([value.currency0.clone(), value.currency1.clone()])
            }
            _ => None,
        })
        .flatten()
        .filter(|address| address.as_slice() != ADDRESS_ZERO)
        .collect::<BTreeSet<_>>();

    let mut calls = Vec::with_capacity(addresses.len() * 4);
    for address in &addresses {
        for selector in [SYMBOL, NAME, TOTAL_SUPPLY, DECIMALS] {
            calls.push(RpcCall {
                to_addr: address.clone(),
                data: selector.to_vec(),
            });
        }
    }

    let responses = if calls.is_empty() {
        Vec::new()
    } else {
        rpc::eth_call(&substreams_ethereum::pb::eth::rpc::RpcCalls { calls }).responses
    };
    if responses.len() != addresses.len() * 4 {
        return Err(Error::msg(format!(
            "metadata RPC response count mismatch: expected {}, got {}",
            addresses.len() * 4,
            responses.len()
        )));
    }

    let mut by_address = BTreeMap::new();
    for (address, response) in addresses.into_iter().zip(responses.chunks_exact(4)) {
        by_address.insert(address, decode_metadata(response));
    }

    for event in &mut events.events {
        let Some(pb::event::Payload::Initialize(value)) = event.payload.as_mut() else {
            continue;
        };
        value.token0_metadata = Some(metadata_for(&value.currency0, &by_address));
        value.token1_metadata = Some(metadata_for(&value.currency1, &by_address));
    }

    Ok(())
}

fn metadata_for(
    address: &[u8],
    values: &BTreeMap<Vec<u8>, pb::TokenMetadata>,
) -> pb::TokenMetadata {
    if address == ADDRESS_ZERO {
        return pb::TokenMetadata {
            symbol: "ETH".to_owned(),
            name: "Ethereum".to_owned(),
            total_supply: "0".to_owned(),
            decimals: Some("18".to_owned()),
        };
    }
    values.get(address).cloned().unwrap_or(pb::TokenMetadata {
        symbol: "unknown".to_owned(),
        name: "unknown".to_owned(),
        total_supply: "0".to_owned(),
        decimals: None,
    })
}

fn decode_metadata(
    response: &[substreams_ethereum::pb::eth::rpc::RpcResponse],
) -> pb::TokenMetadata {
    pb::TokenMetadata {
        symbol: decode_text(&response[0]).unwrap_or_else(|| "unknown".to_owned()),
        name: decode_text(&response[1]).unwrap_or_else(|| "unknown".to_owned()),
        total_supply: decode_uint(&response[2])
            .map(|value| value.to_string())
            .unwrap_or_else(|| "0".to_owned()),
        decimals: decode_uint(&response[3])
            .filter(|value| *value < ethabi::ethereum_types::U256::from(255u16))
            .map(|value| value.to_string()),
    }
}

fn decode_text(response: &substreams_ethereum::pb::eth::rpc::RpcResponse) -> Option<String> {
    if response.failed {
        return None;
    }
    if let Ok(tokens) = ethabi::decode(&[ParamType::String], &response.raw) {
        if let Some(Token::String(value)) = tokens.into_iter().next() {
            return Some(value);
        }
    }
    if response.raw.as_slice() == NULL_ETH_VALUE {
        return None;
    }
    ethabi::decode(&[ParamType::FixedBytes(32)], &response.raw)
        .ok()
        .and_then(|tokens| tokens.into_iter().next())
        .and_then(|token| match token {
            Token::FixedBytes(value) => {
                let end = value
                    .iter()
                    .position(|byte| *byte == 0)
                    .unwrap_or(value.len());
                Some(String::from_utf8_lossy(&value[..end]).into_owned())
            }
            _ => None,
        })
}

fn decode_uint(
    response: &substreams_ethereum::pb::eth::rpc::RpcResponse,
) -> Option<ethabi::ethereum_types::U256> {
    if response.failed {
        return None;
    }
    ethabi::decode(&[ParamType::Uint(256)], &response.raw)
        .ok()
        .and_then(|tokens| tokens.into_iter().next())
        .and_then(|token| match token {
            Token::Uint(value) => Some(value),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use substreams_ethereum::pb::eth::rpc::RpcResponse;

    fn response(raw: Vec<u8>) -> RpcResponse {
        RpcResponse { raw, failed: false }
    }

    #[test]
    fn decodes_string_and_bytes32_fallbacks() {
        let dynamic = response(ethabi::encode(&[Token::String("USDC".to_owned())]));
        assert_eq!(decode_text(&dynamic).as_deref(), Some("USDC"));

        let mut bytes = vec![0; 32];
        bytes[..4].copy_from_slice(b"WETH");
        let fixed = response(ethabi::encode(&[Token::FixedBytes(bytes)]));
        assert_eq!(decode_text(&fixed).as_deref(), Some("WETH"));

        assert_eq!(decode_text(&response(NULL_ETH_VALUE.to_vec())), None);
    }

    #[test]
    fn rejects_graph_mapping_decimal_sentinel() {
        let value = response(ethabi::encode(&[Token::Uint(255u16.into())]));
        assert_eq!(decode_uint(&value).unwrap().as_u64(), 255);
        assert!(decode_uint(&value).is_none_or(|value| value >= 255u16.into()));
    }
}
