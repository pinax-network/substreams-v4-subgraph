//! Stateful entity reconstruction is implemented as the next parity milestone.
//!
//! Keeping the state boundary separate prevents Graph Node entity semantics from
//! leaking into raw event extraction or the eventual Parquet encoder.
