use std::fmt;

#[derive(Debug)]
pub enum FerretError {
    NotFound(String),
    InvalidInput(String),
    Storage(String),
}

impl fmt::Display for FerretError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(s) => write!(f, "not found: {s}. Check the identifier and try again."),
            Self::InvalidInput(s) => write!(
                f,
                "invalid input: {s}. Provide a valid value and try again."
            ),
            Self::Storage(s) => write!(
                f,
                "storage error: {s}. Check the database path and try again."
            ),
        }
    }
}

impl std::error::Error for FerretError {}
