//! Graph Node-native Parquet encoding is implemented after entity-state parity.
//!
//! This module is deliberately separate from raw event extraction so the same
//! deterministic entity changes can feed differential tests and Parquet output.
