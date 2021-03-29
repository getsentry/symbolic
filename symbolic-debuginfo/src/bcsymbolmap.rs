//! The Apple [`BCSymbolMap`] file format.
//!
//! The [`BCSymbolMap`] is used in Apple bitcode builds when symbols are obfuscated.  In
//! this case the actual symbol names are replaced with `__hidden#[0-9]+_` in the binary and
//! dSYMs.  These can then be looked up in the BCSymbolMap to get the original symbol name
//! back.
//!
//! See [`MachObject::load_symbolmap`](crate::macho::MachObject::load_symbolmap) for an
//! example of how to use this.

use std::error::Error;
use std::fmt;
use std::iter::FusedIterator;

use thiserror::Error;

use crate::macho::SWIFT_HIDDEN_PREFIX;

const BC_SYMBOL_MAP_HEADER: &str = "BCSymbolMap Version: 2.0";

/// The error type for handling a [`BCSymbolMap`].
#[derive(Debug, Error)]
#[error("{kind}")]
pub struct BCSymbolMapError {
    kind: BCSymbolMapErrorKind,
    #[source]
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
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

/// An in-memory representation of the Apple bitcode symbol map.
///
/// This is an auxiliary file, not an object file.  It can be used to provide de-obfuscated
/// symbol names to a [`MachObject`] object using its
/// [`load_symbolmap`](crate::macho::MachObject::load_symbolmap) method.
///
/// It is common for bitcode builds to obfuscate the names in the object file's symbol table
/// so that even the DWARF files do not have the actual symbol names.  In this case the
/// build process will create a `.bcsymbolmap` file which maps the obfuscated symbol names
/// back to the original ones.  This structure can parse these files and allows providing
/// this information to the [`MachObject`] so that it has the original symbol names instead
/// of `__hidden#NNN_` ones.
///
/// [`MachObject`]: crate::macho::MachObject
#[derive(Clone, Debug)]
pub struct BCSymbolMap<'d> {
    names: Vec<&'d str>,
}

impl From<BCSymbolMapErrorKind> for BCSymbolMapError {
    fn from(source: BCSymbolMapErrorKind) -> Self {
        Self {
            kind: source,
            source: None,
        }
    }
}

impl<'d> BCSymbolMap<'d> {
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
    pub fn parse(data: &'d [u8]) -> Result<Self, BCSymbolMapError> {
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

        let names = lines_iter.collect();

        Ok(Self { names })
    }

    /// Returns the name of a symbol if it exists in this mapping.
    pub fn get(&self, index: usize) -> Option<&'d str> {
        self.names.get(index).copied()
    }

    /// Resolves a name using this mapping.
    ///
    /// If the name matches the `__hidden#NNN_` pattern that indicates a [`BCSymbolMap`]
    /// lookup it will be looked up the resolved name will be returned.  Otherwise the name
    /// is returned unchanged.
    pub fn resolve(&self, mut name: &'d str) -> &'d str {
        if let Some(tail) = name.strip_prefix(SWIFT_HIDDEN_PREFIX) {
            if let Some(index_as_string) = tail.strip_suffix('_') {
                name = index_as_string
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| self.get(index))
                    .unwrap_or(name);
            }
        }
        name
    }

    /// Returns an iterator over all the names in this bitcode symbol map.
    pub fn iter(&self) -> BCSymbolMapIterator<'_, 'd> {
        BCSymbolMapIterator {
            iter: self.names.iter(),
        }
    }
}

/// Iterator over the names in a [`BCSymbolMap`].
///
/// This struct is created by [`BCSymbolMap::iter`].
pub struct BCSymbolMapIterator<'a, 'd> {
    iter: std::slice::Iter<'a, &'d str>,
}

impl<'a, 'd> Iterator for BCSymbolMapIterator<'a, 'd> {
    type Item = &'d str;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().copied()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl FusedIterator for BCSymbolMapIterator<'_, '_> {}

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
        let data = std::fs::read_to_string(
            "tests/fixtures/c8374b6d-6e96-34d8-ae38-efaa5fec424f.bcsymbolmap",
        )
        .unwrap();

        assert!(BCSymbolMap::test(&data.as_bytes()[..20]));

        let map = BCSymbolMap::parse(data.as_bytes()).unwrap();
        assert_eq!(map.get(2), Some("-[SentryMessage serialize]"))
    }

    #[test]
    fn test_iter() {
        let data = std::fs::read_to_string(
            "tests/fixtures/c8374b6d-6e96-34d8-ae38-efaa5fec424f.bcsymbolmap",
        )
        .unwrap();
        let map = BCSymbolMap::parse(data.as_bytes()).unwrap();

        let mut map_iter = map.iter();

        let (lower_bound, upper_bound) = map_iter.size_hint();
        assert!(lower_bound > 0);
        assert!(upper_bound.is_some());

        let name = map_iter.next();
        assert_eq!(name.unwrap(), "-[SentryMessage initWithFormatted:]");

        let name = map_iter.next();
        assert_eq!(name.unwrap(), "-[SentryMessage setMessage:]");
    }

    #[test]
    fn test_data_lifetime() {
        let data = std::fs::read_to_string(
            "tests/fixtures/c8374b6d-6e96-34d8-ae38-efaa5fec424f.bcsymbolmap",
        )
        .unwrap();

        let name = {
            let map = BCSymbolMap::parse(data.as_bytes()).unwrap();
            map.get(0).unwrap()
        };

        assert_eq!(name, "-[SentryMessage initWithFormatted:]");
    }

    #[test]
    fn test_resolve() {
        let data = std::fs::read_to_string(
            "tests/fixtures/c8374b6d-6e96-34d8-ae38-efaa5fec424f.bcsymbolmap",
        )
        .unwrap();
        let map = BCSymbolMap::parse(data.as_bytes()).unwrap();

        assert_eq!(map.resolve("normal_name"), "normal_name");
        assert_eq!(map.resolve("__hidden#2_"), "-[SentryMessage serialize]");
    }
}
