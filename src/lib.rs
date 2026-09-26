//! Trip Archive — a self-hosted komoot organization replacement.
//!
//! The server crate: the Axum JSON API and the SQLite store behind it, plus
//! the laptop commands (`backup`, `qmapshack_export`, the Komoot tools) under
//! `src/bin`. The UI is the Dioxus SPA in `crates/ui-dioxus` (ADR-0024),
//! which this server serves at `/app`. What is built, and why, is in
//! `docs/requirements.md` and `docs/adr/`.

pub mod config;
pub mod server;

/// The shared data models, re-exported so this crate's `crate::models::…`
/// paths keep working now that the types live in their own wasm-safe crate
/// (`crates/types`, ADR-0024) for the Rust UI to share.
pub use trip_archive_types as models;
