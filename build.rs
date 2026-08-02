use anyhow::Result;
use serde_json::Value;
use std::{env, fs, path::PathBuf};
use substreams_ethereum::Abigen;

const DEPLOYMENT: &str = "Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB";

fn main() -> Result<()> {
    let output = PathBuf::from(env::var("OUT_DIR")?);
    let contracts: [(&str, &str, &[&str]); 3] = [
        (
            "pool_manager",
            "PoolManager.json",
            &["Initialize", "ModifyLiquidity", "Swap"],
        ),
        (
            "position_manager",
            "PositionManager.json",
            &["Subscription", "Unsubscription", "Transfer"],
        ),
        (
            "arrakis_hook_factory",
            "ArrakisHookFactory.json",
            &["LogCreatePrivateHook"],
        ),
    ];

    for (module, filename, event_names) in contracts {
        let abi = format!("artifacts/deployment/{DEPLOYMENT}/abis/{filename}");
        println!("cargo:rerun-if-changed={abi}");
        let filtered_abi = output.join(format!("{module}.events.json"));
        write_event_only_abi(&abi, &filtered_abi, event_names)?;
        Abigen::new(module, filtered_abi.to_str().expect("UTF-8 output path"))?
            .generate()?
            .write_to_file(output.join(format!("{module}.rs")))?;
    }

    Ok(())
}

fn write_event_only_abi(source: &str, output: &PathBuf, event_names: &[&str]) -> Result<()> {
    let entries: Vec<Value> = serde_json::from_str(&fs::read_to_string(source)?)?;
    let filtered = entries
        .into_iter()
        .filter(|entry| {
            entry.get("type").and_then(Value::as_str) == Some("event")
                && entry
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| event_names.contains(&name))
        })
        .collect::<Vec<_>>();
    anyhow::ensure!(
        filtered.len() == event_names.len(),
        "expected {} selected events in {source}, found {}",
        event_names.len(),
        filtered.len()
    );
    fs::write(output, serde_json::to_vec(&filtered)?)?;
    Ok(())
}
