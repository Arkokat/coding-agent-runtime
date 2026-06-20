//! Mutating JSON-RPC handlers for the control UDS.
//!
//! `mutate::dispatch` is the entry point. Read handlers live in
//! `crate::handlers::read` (added by Task 14).

pub mod mutate;
pub mod plugin_handlers;
pub mod read;
pub mod router;
pub mod subscriber_registry;
