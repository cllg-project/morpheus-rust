use thiserror::Error;

#[derive(Debug, Error)]
pub enum MorpheusError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Stemlib load error: {0}")]
    StemlibLoad(String),

    #[error("Beta-code conversion error: {0}")]
    BetaCode(String),

    #[error("Parse error in {file} at line {line}: {msg}")]
    Parse { file: String, line: usize, msg: String },
}

pub type Result<T> = std::result::Result<T, MorpheusError>;
