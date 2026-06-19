use serde_json::Value;
use std::io::{self, BufRead, Write};
use thiserror::Error;

/// Max bytes per NDJSON line. Larger frames are rejected as
/// `FramingError::LineTooLong`. Matches spec section 10.
pub const MAX_LINE_BYTES: usize = 1_048_576; // 1 MiB

#[derive(Debug, Error)]
pub enum FramingError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("line too long (>{max} bytes)", max = MAX_LINE_BYTES)]
    LineTooLong,
    #[error("invalid json: {0}")]
    Json(#[from] serde_json::Error),
}

/// Read one NDJSON message. Returns `None` at EOF (clean connection close).
pub fn read_message<R: BufRead>(r: &mut R) -> Option<Result<Value, FramingError>> {
    let mut buf = Vec::with_capacity(256);
    match r.read_until(b'\n', &mut buf) {
        Ok(0) => None,
        Ok(_) => {
            if buf.len() > MAX_LINE_BYTES {
                return Some(Err(FramingError::LineTooLong));
            }
            while matches!(buf.last(), Some(b'\n' | b'\r')) {
                buf.pop();
            }
            Some(serde_json::from_slice(&buf).map_err(Into::into))
        }
        Err(e) => Some(Err(FramingError::Io(e))),
    }
}

/// Write one NDJSON message: JSON value + a single trailing `\n`.
/// Does not flush; caller controls that.
pub fn write_message<W: Write>(w: &mut W, msg: &Value) -> Result<(), FramingError> {
    let bytes = serde_json::to_vec(msg)?;
    w.write_all(&bytes)?;
    w.write_all(b"\n")?;
    Ok(())
}
