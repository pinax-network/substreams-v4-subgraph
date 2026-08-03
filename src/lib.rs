#![allow(clippy::not_unsafe_ptr_arg_deref)]

mod abi;
mod config;
mod extract;
mod metadata;
pub mod pb;

use pb::pinax::uniswap::v4::base::v1::Events;
use substreams::errors::Error;
use substreams_ethereum::pb::eth::v2::Block;

substreams_ethereum::init!();

#[substreams::handlers::map]
pub fn map_events(params: String, block: Block) -> Result<Events, Error> {
    let config = config::Config::parse(&params)?;
    let mut events = extract::map_block(&config, &block);
    metadata::enrich(&mut events)?;
    Ok(events)
}
