//! CalmMD Rust core: services, DTOs and the local MCP server.
//!
//! This crate is UI-framework agnostic. Both the Tauri compatibility shell and
//! the GPUI Kit shell construct an [`AppContext`] and call the same services.

pub mod app;
pub mod core;
pub mod fsops;
pub mod kv;
pub mod mcp;
pub mod search;
pub mod tree;
pub mod windows;

pub use app::{AppContext, EventSink, NullEvents, WindowInfo, WindowRegistry};
