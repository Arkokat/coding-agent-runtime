//! agentd-plugin-opencode — opencode bridge plugin for the agentd daemon.
//!
//! Modules:
//! - [`discovery`] — tmux pane discovery (which panes are running `opencode`).
//! - [`watcher`] — poll pane content for status, emit events to the daemon.

#![warn(missing_docs)]

pub mod discovery;
pub mod watcher;
