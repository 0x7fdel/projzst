use thiserror::Error;

/// Custom error types for projzst operations
#[derive(Error, Debug)]
pub enum ProjzstError {
    /// IO operation failed
    #[error("IO operation failed: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization failed
    #[error("JSON parsing failed: {0}")]
    Json(#[from] serde_json::Error),

    /// MessagePack encoding failed
    #[error("MessagePack encoding failed: {0}")]
    MsgPackEncode(#[from] rmp_serde::encode::Error),

    /// MessagePack decoding failed
    #[error("MessagePack decoding failed: {0}")]
    MsgPackDecode(#[from] rmp_serde::decode::Error),

    /// Metadata frame size exceeds MAX_METADATA_SIZE
    #[error("Invalid metadata length: got {0} bytes")]
    InvalidMetadataLength(usize),

    /// Extra metadata file specified but not found
    #[error("Extra metadata file not found: {0}")]
    ExtraFileNotFound(String),

    /// Source directory to pack does not exist
    #[error("Source directory does not exist: {0}")]
    SourceNotFound(String),

    /// File header is invalid
    #[error("Failed to read file header or invalid file format")]
    InvalidFileHeader,
}

/// Result type alias for projzst operations
pub type Result<T> = std::result::Result<T, ProjzstError>;
