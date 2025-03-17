use thiserror::Error;

/// Application error types
#[derive(Error, Debug)]
pub enum AppError {
    #[error("GDB error: {0}")]
    GDBError(String),

    #[error("GDB timeout")]
    GDBTimeout,

    #[error("GDB busy")]
    GDBBusy,

    #[error("GDB quit")]
    GDBQuit,

    #[error("Parse error: {0}")]
    #[allow(dead_code)]
    ParseError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
}

/// Application result type
pub type AppResult<T> = Result<T, AppError>;
