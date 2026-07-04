//! Trip Archive — a self-hosted komoot organization replacement.
//!
//! This crate currently implements **US-1: import a GPX file** (see
//! `docs/requirements.md`). The HTTP surface is a plain Axum app; the Leptos
//! front-end described in ADR-0001 arrives in a later milestone.

pub mod config;
pub mod models;
pub mod server;
