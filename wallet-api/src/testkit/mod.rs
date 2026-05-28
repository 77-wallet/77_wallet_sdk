//! Crate-side test kit for integration tests.
//!
//! `tests/harness` owns the external test environment. `testkit` exposes
//! intentional test-only entrypoints into internal wallet-api workflows.

pub mod adapter_factory;
pub mod collect;
pub mod collect_fee;
pub mod sol_transaction;
pub mod withdraw;
