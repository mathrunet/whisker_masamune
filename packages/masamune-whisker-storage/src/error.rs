/// Failure of a storage operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    /// No object at the path.
    NotFound,
    /// Not signed in, or the storage rules denied the operation.
    Unauthorized,
    /// The platform or adapter cannot do this (for example Firebase Storage on the web).
    Unsupported(String),
    /// Bad path or arguments.
    InvalidArgument(String),
    /// Reading or writing a local file failed.
    Io(String),
    /// The server could not be reached or is temporarily failing; retrying may succeed.
    Unavailable(String),
    /// Any other backend error.
    Backend { code: String, message: String },
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageError::NotFound => f.write_str("object not found"),
            StorageError::Unauthorized => f.write_str("unauthorized"),
            StorageError::Unsupported(message) => write!(f, "unsupported: {message}"),
            StorageError::InvalidArgument(message) => write!(f, "invalid argument: {message}"),
            StorageError::Io(message) => write!(f, "io: {message}"),
            StorageError::Unavailable(message) => write!(f, "unavailable: {message}"),
            StorageError::Backend { code, message } => write!(f, "{code}: {message}"),
        }
    }
}

impl std::error::Error for StorageError {}

impl StorageError {
    /// Whether retrying may succeed.
    pub fn is_transient(&self) -> bool {
        matches!(self, StorageError::Unavailable(_))
    }
}

impl From<std::io::Error> for StorageError {
    fn from(error: std::io::Error) -> Self {
        StorageError::Io(error.to_string())
    }
}

pub type StorageResult<T> = Result<T, StorageError>;
