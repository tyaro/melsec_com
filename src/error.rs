use thiserror::Error;

#[derive(Error, Debug)]
pub enum MelsecError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("timeout")]
    Timeout,

    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("already registered")]
    AlreadyRegistered,
    #[error("no connection target set on client")]
    NoTarget,
}
