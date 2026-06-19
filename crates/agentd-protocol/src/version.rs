/// Current protocol version. Bump on breaking IPC changes.
///
/// Forward-compat: daemon accepts plugins with `PROTOCOL_VERSION` <= daemon's
/// (with warning). Plugins with `PROTOCOL_VERSION` > daemon's are rejected.
pub const PROTOCOL_VERSION: u32 = 1;
