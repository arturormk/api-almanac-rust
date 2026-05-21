use thiserror::Error;

#[derive(Debug, Error)]
pub enum RunnerError {
    #[error("Invalid HTTP method '{0}'")]
    InvalidMethod(String),

    #[error("Request failed: {0}")]
    Request(#[from] reqwest::Error),
}
