use std::fmt::{Display, Formatter};
use std::io;
use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, MdxError>;

#[derive(Debug)]
pub enum MdxError {
    InvalidInput(String),
    InvalidFormat(String),
    Unsupported(String),
    IndexMiss {
        dictionary: String,
        keyword: String,
    },
    DictionaryNotFound(String),
    UnsafeAssetPath(String),
    AssetNotFound(String),
    Io {
        path: Option<PathBuf>,
        source: io::Error,
    },
}

impl MdxError {
    pub fn io(path: impl Into<Option<PathBuf>>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

impl Display for MdxError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            Self::InvalidFormat(msg) => write!(f, "invalid MDict format: {msg}"),
            Self::Unsupported(msg) => write!(f, "unsupported MDict feature: {msg}"),
            Self::IndexMiss {
                dictionary,
                keyword,
            } => {
                write!(f, "index miss in dictionary '{dictionary}' for '{keyword}'")
            }
            Self::DictionaryNotFound(name) => write!(f, "dictionary not found: {name}"),
            Self::UnsafeAssetPath(path) => write!(f, "unsafe asset path: {path}"),
            Self::AssetNotFound(path) => write!(f, "asset not found: {path}"),
            Self::Io {
                path: Some(path),
                source,
            } => write!(f, "io error at {}: {source}", path.display()),
            Self::Io { path: None, source } => write!(f, "io error: {source}"),
        }
    }
}

impl std::error::Error for MdxError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<io::Error> for MdxError {
    fn from(source: io::Error) -> Self {
        Self::Io { path: None, source }
    }
}
