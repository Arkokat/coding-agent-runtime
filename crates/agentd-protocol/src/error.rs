use serde::{Deserialize, Serialize};
use std::fmt;

/// All errors that can cross the IPC boundary.
///
/// Each variant maps to a JSON-RPC 2.0 error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProtocolError {
    /// Invalid JSON was received. Code: -32700.
    ParseError,
    /// The JSON sent is not a valid Request object. Code: -32600.
    InvalidRequest,
    /// The method does not exist or is not available. Code: -32601.
    MethodNotFound,
    /// Invalid method parameters. Code: -32602.
    InvalidParams,
    /// Internal JSON-RPC error. Code: -32603.
    InternalError,
    /// Session not found. Code: -32001.
    SessionNotFound,
    /// Plugin binary not in `plugins.toml` allowlist. Code: -32002.
    PluginNotAllowed,
    /// Peer uid does not match daemon's uid. Code: -32003.
    PermissionDenied,
    /// Plugin is not authoritative for the session it claimed. Code: -32004.
    PluginNotAuthoritative,
    /// Daemon is shutting down, request rejected. Code: -32005.
    DaemonShuttingDown,
}

impl ProtocolError {
    /// Return the JSON-RPC numeric code for this error.
    pub const fn code(&self) -> i32 {
        match self {
            ProtocolError::ParseError => -32700,
            ProtocolError::InvalidRequest => -32600,
            ProtocolError::MethodNotFound => -32601,
            ProtocolError::InvalidParams => -32602,
            ProtocolError::InternalError => -32603,
            ProtocolError::SessionNotFound => -32001,
            ProtocolError::PluginNotAllowed => -32002,
            ProtocolError::PermissionDenied => -32003,
            ProtocolError::PluginNotAuthoritative => -32004,
            ProtocolError::DaemonShuttingDown => -32005,
        }
    }

    /// Default human-readable message.
    pub const fn default_message(&self) -> &'static str {
        match self {
            ProtocolError::ParseError => "parse error",
            ProtocolError::InvalidRequest => "invalid request",
            ProtocolError::MethodNotFound => "method not found",
            ProtocolError::InvalidParams => "invalid params",
            ProtocolError::InternalError => "internal error",
            ProtocolError::SessionNotFound => "session not found",
            ProtocolError::PluginNotAllowed => "plugin not in allowlist",
            ProtocolError::PermissionDenied => "permission denied",
            ProtocolError::PluginNotAuthoritative => "plugin not authoritative for session",
            ProtocolError::DaemonShuttingDown => "daemon shutting down",
        }
    }

    /// Attach a custom message, returning a `ProtocolErrorWithMessage` for serialization.
    pub fn with_message(self, msg: impl Into<String>) -> ProtocolErrorWithMessage {
        ProtocolErrorWithMessage {
            code: self.code(),
            message: msg.into(),
        }
    }
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code(), self.default_message())
    }
}

impl std::error::Error for ProtocolError {}

/// A `ProtocolError` with a custom message, for JSON-RPC responses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolErrorWithMessage {
    /// The numeric error code.
    pub code: i32,
    /// The human-readable message.
    pub message: String,
}

impl fmt::Display for ProtocolErrorWithMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ProtocolErrorWithMessage {}
