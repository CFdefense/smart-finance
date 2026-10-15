//! Smart Finance API — library crate.
//!
//! Exposes application modules for use by integration tests and helper binaries.

pub mod controllers;
pub mod db;
pub mod error;
pub mod global;
pub mod log;
pub mod middleware;
pub mod models;
pub mod swagger;

#[cfg(test)]
mod tests;
