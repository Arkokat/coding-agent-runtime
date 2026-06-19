use thiserror::Error;

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("not yet implemented")]
    Stub,
}
