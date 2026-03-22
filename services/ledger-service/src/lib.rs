// Library target — exposes internal modules so that integration tests and
// Criterion benchmarks can import domain types without re-declaring them.
#![allow(dead_code)]

pub mod api;
pub mod application;
pub mod config;
pub mod domain;
pub mod infrastructure;
