//! What can go wrong while reading an IR tree.

use std::fmt;
use std::io;
use std::path::PathBuf;

/// Reading an IR tree failed.
///
/// Every variant names the file it is about: a build reads 436 files, and an
/// error that does not say which one costs a bisection.
#[derive(Debug)]
pub enum Error {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    /// The IR is older than this reader understands.
    Schema {
        /// The file or entry the version came from.
        what: String,
        found: u32,
        required: u32,
    },
    /// The IR was written with ablations: parts of schema 4 are missing on
    /// purpose, and rendering it would produce a page that looks fine and is
    /// wrong.
    Ablated {
        ablations: Vec<String>,
    },
    /// A module file names a module other than the one the index filed it
    /// under. An incremental merge that copied the wrong file looks like this.
    ModuleMismatch {
        path: PathBuf,
        expected: String,
        found: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "reading {}: {source}", path.display()),
            Self::Json { path, source } => write!(f, "parsing {}: {source}", path.display()),
            Self::Schema {
                what,
                found,
                required,
            } => write!(
                f,
                "{what} is schema {found}; this reader needs schema {required} or newer \
                 (re-extract with --tagged-code)"
            ),
            Self::Ablated { ablations } => write!(
                f,
                "this IR was written with ablations [{}] and is incomplete on purpose; \
                 it is for the stopwatch only",
                ablations.join(", ")
            ),
            Self::ModuleMismatch {
                path,
                expected,
                found,
            } => write!(
                f,
                "{} declares module {found}, but the index files it under {expected}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
