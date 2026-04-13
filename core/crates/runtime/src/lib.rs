pub mod context;
pub mod prompt;
pub mod react;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("model error: {0}")]
    Model(#[from] velkor_models::ProviderError),
    #[error("tool error: {0}")]
    Tool(#[from] velkor_tools::ToolError),
    #[error("memory error: {0}")]
    Memory(#[from] velkor_memory::MemoryError),
    #[error("max iterations ({0}) exceeded — possible infinite tool loop")]
    MaxIterations(u32),
    #[error("{0}")]
    Other(String),
}
