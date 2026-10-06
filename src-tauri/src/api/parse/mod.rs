//! Pure parsing helpers, split by concern.
//!
//! Nothing in here performs I/O: every function takes an already-fetched body
//! (or a parsed [`serde_json::Value`]) and returns domain values. That keeps the
//! "parse" step testable without a server, as required by the spec (§11).

pub mod metrics;
pub mod model;
pub mod prometheus;
pub mod slots;
