//! The Apple BCSymbolMap file format.
//!
//! The BCSymbolMap is used in Apple bitcode builds when symbols are obfuscated.  In this
//! case the actual symbol names are replaced with `__hidden#[0-9]+` in the binary and
//! dSYMs.  These can then be looked up in the BCSymbolMap to get the original symbol name
//! back.

use std::error::Error as StdError;
use std::fmt;

use thiserror::Error;

use symbolic_common::DebugId;

const BC_SYMBOL_MAP_HEADER: &str = "BCSymbolMap Version: 2.0";

/// The error type for handling a [`BCSymbolMap`].
#[derive(Debug, Error)]
#[error("{kind}")]
pub struct BCSymbolMapError {
    kind: BCSymbolMapErrorKind,
    #[source]
    source: Option<Box<dyn StdError + Send + Sync + 'static>>,
}

/// Error kind for [`BCSymbolMapError`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BCSymbolMapErrorKind {
    /// The BCSymbolMap header does not match a supported version.
    ///
    /// It could be entirely missing, or only be an unknown version or otherwise corrupted.
    InvalidHeader,
    /// The bitcode symbol map did contain invalid UTF-8.
    InvalidUtf8,
}

impl fmt::Display for BCSymbolMapErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::InvalidHeader => write!(f, "no valid BCSymbolMap header was found"),
            Self::InvalidUtf8 => write!(f, "BCSymbolmap is not valid UTF-8"),
        }
    }
}

/// An in-memory representation of the BCSymbolMap.
///
/// This is an auxiliary file, not an object file.
///
/// This can be used to provide symbols to a [`MachO`](crate::macho::MachO) object.
///
/// TODO(flub): Make this not own the data.
#[derive(Clone, Debug)]
pub struct BCSymbolMap {
    id: DebugId,
    names: Vec<String>,
}

impl From<BCSymbolMapErrorKind> for BCSymbolMapError {
    fn from(source: BCSymbolMapErrorKind) -> Self {
        Self {
            kind: source,
            source: None,
        }
    }
}

impl BCSymbolMap {
    /// Tests whether the buffer could contain a [`BCSymbolMap`].
    pub fn test(bytes: &[u8]) -> bool {
        let mut pattern = BC_SYMBOL_MAP_HEADER.as_bytes();
        if pattern.len() > bytes.len() {
            pattern = &pattern[..bytes.len()];
        }
        bytes.starts_with(pattern)
    }

    /// Parses the BCSymbolMap.
    ///
    /// A symbol map does not contain the UUID of its symbols, instead this is normally
    /// encoded in the filename.
    pub fn parse(id: DebugId, data: &[u8]) -> Result<Self, BCSymbolMapError> {
        let content = std::str::from_utf8(data).map_err(|err| BCSymbolMapError {
            kind: BCSymbolMapErrorKind::InvalidUtf8,
            source: Some(Box::new(err)),
        })?;

        let mut lines_iter = content.lines();

        let header = lines_iter
            .next()
            .ok_or(BCSymbolMapErrorKind::InvalidHeader)?;
        if header != BC_SYMBOL_MAP_HEADER {
            return Err(BCSymbolMapErrorKind::InvalidHeader.into());
        }

        let mut names = Vec::new();
        for line in lines_iter {
            names.push(line.to_string());
        }

        Ok(Self { id, names })
    }

    /// Returns the name of a symbol if it exists in this mapping.
    pub fn get(&self, index: usize) -> Option<&str> {
        self.names.get(index).map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bcsymbolmap_test() {
        let buf = b"BCSymbolMap Vers";
        assert!(BCSymbolMap::test(&buf[..]));

        let buf = b"oops";
        assert!(!BCSymbolMap::test(&buf[..]));
    }

    #[test]
    fn test_basic() {
        let uuid: DebugId = "c8374b6d-6e96-34d8-ae38-efaa5fec424F".parse().unwrap();
        let data = std::fs::read_to_string(
            "tests/fixtures/c8374b6d-6e96-34d8-ae38-efaa5fec424F.bcsymbolmap",
        )
        .unwrap();

        assert!(BCSymbolMap::test(&data.as_bytes()[..20]));

        let map = BCSymbolMap::parse(uuid, data.as_bytes()).unwrap();
        assert_eq!(map.get(2), Some("-[SentryMessage serialize]"))
    }
}
