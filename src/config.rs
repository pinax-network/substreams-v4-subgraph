use std::collections::BTreeMap;

use substreams::errors::Error;

pub const NETWORK: &str = "base";
pub const DEPLOYMENT: &str = "Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB";
pub const GRAFT_BASE: &str = "QmS1ehFzXTD9eA1f1EgjZvdyAj2EHtVNMrEN91H3pLuHMy";
pub const GRAFT_BLOCK: u64 = 26_990_278;
pub const POOL_MANAGER: &str = "498581ff718922c3f8e6a244956af099b2652b2b";
pub const POOL_MANAGER_START: u64 = 25_350_988;
pub const POSITION_MANAGER: &str = "7c5f5a4bbd8fd63184577525326123b519429bdc";
pub const POSITION_MANAGER_START: u64 = 25_350_993;
pub const ARRAKIS_HOOK_FACTORY: &str = "ef129a430032c8183aba158c1a70799e3b840df9";
pub const ARRAKIS_HOOK_FACTORY_START: u64 = 28_450_225;
#[cfg(test)]
pub const PINNED_PARAMS: &str = "network=base;deployment=Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB;graft_base=QmS1ehFzXTD9eA1f1EgjZvdyAj2EHtVNMrEN91H3pLuHMy;graft_block=26990278;pool_manager=0x498581ff718922c3f8e6a244956af099b2652b2b;pool_manager_start=25350988;position_manager=0x7c5f5a4bbd8fd63184577525326123b519429bdc;position_manager_start=25350993;arrakis_hook_factory=0xef129a430032c8183aba158c1a70799e3b840df9;arrakis_hook_factory_start=28450225";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub pool_manager: Vec<u8>,
    pub position_manager: Vec<u8>,
    pub arrakis_hook_factory: Vec<u8>,
}

impl Config {
    pub fn parse(input: &str) -> Result<Self, Error> {
        let mut values = BTreeMap::new();
        for pair in input.split(';') {
            let (key, value) = pair
                .split_once('=')
                .ok_or_else(|| Error::msg(format!("invalid deployment parameter `{pair}`")))?;
            if values.insert(key, value).is_some() {
                return Err(Error::msg(format!(
                    "duplicate deployment parameter `{key}`"
                )));
            }
        }

        require(&values, "network", NETWORK)?;
        require(&values, "deployment", DEPLOYMENT)?;
        require(&values, "graft_base", GRAFT_BASE)?;
        require(&values, "graft_block", &GRAFT_BLOCK.to_string())?;
        require_address(&values, "pool_manager", POOL_MANAGER)?;
        require(
            &values,
            "pool_manager_start",
            &POOL_MANAGER_START.to_string(),
        )?;
        require_address(&values, "position_manager", POSITION_MANAGER)?;
        require(
            &values,
            "position_manager_start",
            &POSITION_MANAGER_START.to_string(),
        )?;
        require_address(&values, "arrakis_hook_factory", ARRAKIS_HOOK_FACTORY)?;
        require(
            &values,
            "arrakis_hook_factory_start",
            &ARRAKIS_HOOK_FACTORY_START.to_string(),
        )?;

        const EXPECTED_KEYS: usize = 10;
        if values.len() != EXPECTED_KEYS {
            let known = [
                "network",
                "deployment",
                "graft_base",
                "graft_block",
                "pool_manager",
                "pool_manager_start",
                "position_manager",
                "position_manager_start",
                "arrakis_hook_factory",
                "arrakis_hook_factory_start",
            ];
            let unknown = values
                .keys()
                .filter(|key| !known.contains(key))
                .copied()
                .collect::<Vec<_>>();
            return Err(Error::msg(format!(
                "unexpected deployment parameter(s): {}",
                unknown.join(", ")
            )));
        }

        Ok(Self {
            pool_manager: decode_address(POOL_MANAGER),
            position_manager: decode_address(POSITION_MANAGER),
            arrakis_hook_factory: decode_address(ARRAKIS_HOOK_FACTORY),
        })
    }
}

fn require(values: &BTreeMap<&str, &str>, key: &str, expected: &str) -> Result<(), Error> {
    let actual = values
        .get(key)
        .ok_or_else(|| Error::msg(format!("missing deployment parameter `{key}`")))?;
    if actual != &expected {
        return Err(Error::msg(format!(
            "deployment parameter `{key}` must be `{expected}`, got `{actual}`"
        )));
    }
    Ok(())
}

fn require_address(values: &BTreeMap<&str, &str>, key: &str, expected: &str) -> Result<(), Error> {
    let actual = values
        .get(key)
        .ok_or_else(|| Error::msg(format!("missing deployment parameter `{key}`")))?
        .strip_prefix("0x")
        .unwrap_or(values[key]);
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(Error::msg(format!(
            "deployment parameter `{key}` must be `0x{expected}`, got `{}`",
            values[key]
        )));
    }
    Ok(())
}

fn decode_address(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).expect("pinned address is hex"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_the_pinned_deployment() {
        let config = Config::parse(PINNED_PARAMS).unwrap();
        assert_eq!(config.pool_manager.len(), 20);
        assert_eq!(config.position_manager.len(), 20);
        assert_eq!(config.arrakis_hook_factory.len(), 20);
    }

    #[test]
    fn rejects_an_address_override() {
        let params = PINNED_PARAMS.replace(
            "0x498581ff718922c3f8e6a244956af099b2652b2b",
            "0x0000000000000000000000000000000000000000",
        );
        let error = Config::parse(&params).unwrap_err().to_string();
        assert!(error.contains("pool_manager"));
    }

    #[test]
    fn rejects_unknown_parameters() {
        let error = Config::parse(&format!("{PINNED_PARAMS};unsafe_override=true"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("unsafe_override"));
    }
}
