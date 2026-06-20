//! agentd: daemon + CLI for orchestrating coding-agent sessions inside tmux.
//!
//! This crate exposes both a library (for tests and future in-workspace
//! consumers) and a single binary `agentd` whose entry point is
//! `src/main.rs`. The AGENTS.md "no library output" rule is intentionally
//! relaxed to allow `use agentd::...` in integration tests across Plan 2.

pub mod cli;
pub mod config;
pub mod control_client;
pub mod daemon;
pub mod db;
pub mod event_bus;
pub mod handlers;
pub mod ipc;
pub mod paths;
pub mod plugin_spawner;
pub mod plugin_supervisor;
pub mod plugins_manifest;
pub mod session_create;
pub mod state;
pub mod status;
pub mod tmux;
