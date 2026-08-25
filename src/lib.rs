//! Trip Archive — a self-hosted komoot organization replacement.
//!
//! This crate currently implements **US-1: import a GPX file** (see
//! `docs/requirements.md`). The HTTP surface is a plain Axum app; the Leptos
//! front-end described in ADR-0001 arrives in a later milestone.

pub mod config;
pub mod server;

/// The shared data models, re-exported so this crate's `crate::models::…`
/// paths keep working now that the types live in their own wasm-safe crate
/// (`crates/types`) for the Rust UI to share.
pub use trip_archive_types as models;
