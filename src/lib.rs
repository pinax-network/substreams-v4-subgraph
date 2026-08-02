#![allow(clippy::not_unsafe_ptr_arg_deref)]

mod abi;
mod config;
mod extract;
pub mod pb;
pub mod sink;
pub mod state;

use pb::pinax::uniswap::v4::base::v1::Events;
use substreams::errors::Error;
use substreams_ethereum::pb::eth::v2::Block;

substreams_ethereum::init!();

#[substreams::handlers::map]
pub fn map_events(params: String, block: Block) -> Result<Events, Error> {
    let config = config::Config::parse(&params)?;
    Ok(extract::map_block(&config, &block))
}
