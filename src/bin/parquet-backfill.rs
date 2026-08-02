use std::{fs, path::PathBuf};
use substreams_v4_subgraph::{
    sink::{validate_head_hash, DumpBuilder, SinkError},
    snapshot::ReplaySnapshot,
};

struct Args {
    snapshot: PathBuf,
    output: PathBuf,
    head_block: i32,
    head_hash: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = args()?;
    if !validate_head_hash(&args.head_hash) {
        return Err("--head-hash must be 32-byte hexadecimal".into());
    }
    let snapshot: ReplaySnapshot = serde_json::from_slice(&fs::read(&args.snapshot)?)?;
    let metadata = DumpBuilder::from_snapshot(&snapshot)?.write(
        &args.output,
        args.head_block,
        &args.head_hash,
    )?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "deployment": metadata.deployment,
            "head_block": args.head_block,
            "entity_count": metadata.entity_count,
            "output": args.output,
        }))?
    );
    Ok(())
}

fn args() -> Result<Args, SinkError> {
    let mut values = std::env::args_os().skip(1);
    let mut snapshot = None;
    let mut output = None;
    let mut head_block = None;
    let mut head_hash = None;
    while let Some(flag) = values.next() {
        let value = values.next().ok_or_else(|| {
            SinkError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("{} requires a value", flag.to_string_lossy()),
            ))
        })?;
        match flag.to_string_lossy().as_ref() {
            "--snapshot" => snapshot = Some(PathBuf::from(value)),
            "--output" => output = Some(PathBuf::from(value)),
            "--head-block" => {
                head_block = Some(value.to_string_lossy().parse::<i32>().map_err(|error| {
                    SinkError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, error))
                })?)
            }
            "--head-hash" => head_hash = Some(value.to_string_lossy().into_owned()),
            flag => {
                return Err(SinkError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("unknown argument `{flag}`"),
                )))
            }
        }
    }
    let missing = |name: &str| {
        SinkError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("missing {name}"),
        ))
    };
    Ok(Args {
        snapshot: snapshot.ok_or_else(|| missing("--snapshot"))?,
        output: output.ok_or_else(|| missing("--output"))?,
        head_block: head_block.ok_or_else(|| missing("--head-block"))?,
        head_hash: head_hash.ok_or_else(|| missing("--head-hash"))?,
    })
}
