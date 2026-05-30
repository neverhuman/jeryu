use std::fmt::{Display, Formatter};
use std::path::PathBuf;

pub type RustJetResult<T> = Result<T, RustJetError>;

#[derive(Debug)]
pub enum RustJetError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    MissingWorkspaceManifest(PathBuf),
    ManifestParse {
        path: PathBuf,
        message: String,
    },
    UnknownPackage(String),
    EmptyWorkspace,
    InvalidShardCount,
}

impl RustJetError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub fn parse(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::ManifestParse {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl Display for RustJetError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "I/O error at {}: {source}", path.display()),
            Self::MissingWorkspaceManifest(path) => {
                write!(f, "missing workspace Cargo.toml at {}", path.display())
            }
            Self::ManifestParse { path, message } => {
                write!(f, "could not parse {}: {message}", path.display())
            }
            Self::UnknownPackage(name) => write!(f, "unknown package: {name}"),
            Self::EmptyWorkspace => write!(f, "workspace contains no packages"),
            Self::InvalidShardCount => write!(f, "shard count must be greater than zero"),
        }
    }
}

impl std::error::Error for RustJetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
