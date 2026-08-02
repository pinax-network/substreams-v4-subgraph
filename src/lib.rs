#![allow(clippy::not_unsafe_ptr_arg_deref)]

mod abi;
mod config;
pub mod decimal;
pub mod entities;
mod extract;
mod math;
mod metadata;
pub mod pb;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod poi;
mod reducer;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod sink;
pub mod snapshot;
pub mod state;

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
