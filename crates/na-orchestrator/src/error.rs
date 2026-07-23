use thiserror::Error;

#[allow(dead_code)]
#[derive(Error, Debug)]
pub enum FlowError {
    #[error("cycle detected in flow DAG")]
    CycleDetected,

    #[error("node '{0}' not found")]
    NodeNotFound(String),

    #[error("node '{0}' failed: {1}")]
    NodeFailed(String, String),

    #[error("validation error: {0}")]
    ValidationError(String),

    #[error("vault error: {0}")]
    VaultError(String),

    #[error("schema error: {0}")]
    SchemaError(String),

    #[error("state error: {0}")]
    StateError(String),
}
