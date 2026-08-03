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
    mode: Mode,
    checkpoint: Option<PathBuf>,
    stop_after_tables: Option<usize>,
}

#[derive(Clone, Copy)]
enum Mode {
    Write,
    Append,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = args()?;
    if !validate_head_hash(&args.head_hash) {
        return Err("--head-hash must be 32-byte hexadecimal".into());
    }
    let snapshot: ReplaySnapshot = serde_json::from_slice(&fs::read(&args.snapshot)?)?;
    let builder = DumpBuilder::from_snapshot(&snapshot)?;
    let checkpoint = args
        .checkpoint
        .as_ref()
        .map(|_| builder.resume_checkpoint(&snapshot));
    let metadata = match args.mode {
        Mode::Write => builder.write(&args.output, args.head_block, &args.head_hash)?,
        Mode::Append => builder.append(
            &args.output,
            &snapshot,
            args.head_block,
            &args.head_hash,
            args.stop_after_tables,
        )?,
    };
    if let (Some(path), Some(checkpoint)) = (&args.checkpoint, checkpoint) {
        write_atomic(path, &serde_json::to_vec(&checkpoint)?)?;
    }
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "deployment": metadata.deployment,
            "head_block": args.head_block,
            "entity_count": metadata.entity_count,
            "output": args.output,
            "mode": match args.mode { Mode::Write => "write", Mode::Append => "append" },
            "checkpoint": args.checkpoint,
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
    let mut mode = Mode::Write;
    let mut checkpoint = None;
    let mut stop_after_tables = None;
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
            "--mode" => {
                mode = match value.to_string_lossy().as_ref() {
                    "write" => Mode::Write,
                    "append" => Mode::Append,
                    value => {
                        return Err(SinkError::Io(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            format!("--mode must be write or append, got `{value}`"),
                        )))
                    }
                }
            }
            "--checkpoint" => checkpoint = Some(PathBuf::from(value)),
            "--stop-after-tables" => {
                stop_after_tables =
                    Some(value.to_string_lossy().parse::<usize>().map_err(|error| {
                        SinkError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, error))
                    })?)
            }
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
    if stop_after_tables == Some(0) {
        return Err(SinkError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "--stop-after-tables must be greater than zero",
        )));
    }
    if matches!(mode, Mode::Write) && stop_after_tables.is_some() {
        return Err(SinkError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "--stop-after-tables is valid only with --mode append",
        )));
    }
    Ok(Args {
        snapshot: snapshot.ok_or_else(|| missing("--snapshot"))?,
        output: output.ok_or_else(|| missing("--output"))?,
        head_block: head_block.ok_or_else(|| missing("--head-block"))?,
        head_hash: head_hash.ok_or_else(|| missing("--head-hash"))?,
        mode,
        checkpoint,
        stop_after_tables,
    })
}

fn write_atomic(path: &PathBuf, bytes: &[u8]) -> Result<(), std::io::Error> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)
}
