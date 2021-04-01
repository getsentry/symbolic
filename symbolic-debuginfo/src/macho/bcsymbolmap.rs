//! The Apple [`BcSymbolMap`] file format.

use std::error::Error;
use std::fmt;
use std::io::Cursor;
use std::iter::FusedIterator;
use std::path::Path;

use elementtree::Element;
use symbolic_common::{DebugId, ParseDebugIdError};
use thiserror::Error;

use super::SWIFT_HIDDEN_PREFIX;

const BC_SYMBOL_MAP_HEADER: &str = "BCSymbolMap Version: 2.0";

/// The error type for handling a [`BcSymbolMap`].
#[derive(Debug, Error)]
#[error("{kind}")]
pub struct BcSymbolMapError {
    kind: BcSymbolMapErrorKind,
    #[source]
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

/// Error kind for [`BCSymbolMapError`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BcSymbolMapErrorKind {
    /// The BCSymbolMap header does not match a supported version.
    ///
    /// It could be entirely missing, or only be an unknown version or otherwise corrupted.
    InvalidHeader,
    /// The bitcode symbol map did contain invalid UTF-8.
    InvalidUtf8,
}

impl fmt::Display for BcSymbolMapErrorKind {
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
/// See [`MachObject::load_symbolmap`](crate::macho::MachObject::load_symbolmap) for an
/// example of how to use this.
///
/// [`MachObject`]: crate::macho::MachObject
#[derive(Clone, Debug)]
pub struct BcSymbolMap<'d> {
    names: Vec<&'d str>,
}

impl From<BcSymbolMapErrorKind> for BcSymbolMapError {
    fn from(source: BcSymbolMapErrorKind) -> Self {
        Self {
            kind: source,
            source: None,
        }
    }
}

impl<'d> BcSymbolMap<'d> {
    /// Tests whether the buffer could contain a [`BcSymbolMap`].
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
    pub fn parse(data: &'d [u8]) -> Result<Self, BcSymbolMapError> {
        let content = std::str::from_utf8(data).map_err(|err| BcSymbolMapError {
            kind: BcSymbolMapErrorKind::InvalidUtf8,
            source: Some(Box::new(err)),
        })?;

        let mut lines_iter = content.lines();

        let header = lines_iter
            .next()
            .ok_or(BcSymbolMapErrorKind::InvalidHeader)?;
        if header != BC_SYMBOL_MAP_HEADER {
            return Err(BcSymbolMapErrorKind::InvalidHeader.into());
        }

        let names = lines_iter.collect();

        Ok(Self { names })
    }

    /// Returns the name of a symbol if it exists in this mapping.
    ///
    /// # Examples
    ///
    /// ```
    /// use symbolic_debuginfo::macho::BcSymbolMap;
    ///
    /// // let data = std::fs::read("c8374b6d-6e96-34d8-ae38-efaa5fec424f.bcsymbolmap").unwrap();
    /// # let data =
    /// #     std::fs::read("tests/fixtures/c8374b6d-6e96-34d8-ae38-efaa5fec424f.bcsymbolmap")
    /// #         .unwrap();
    /// let map = BcSymbolMap::parse(&data).unwrap();
    ///
    /// assert_eq!(map.get(43), Some("Sources/Sentry/Public/SentryMessage.h"));
    /// assert_eq!(map.get(usize::MAX), None);  // We do not have this many entries
    /// ```
    pub fn get(&self, index: usize) -> Option<&'d str> {
        self.names.get(index).copied()
    }

    /// Resolves a name using this mapping.
    ///
    /// If the name matches the `__hidden#NNN_` pattern that indicates a [`BcSymbolMap`]
    /// lookup it will be looked up the resolved name will be returned.  Otherwise the name
    /// is returned unchanged.
    ///
    /// # Examples
    ///
    /// ```
    /// use symbolic_debuginfo::macho::BcSymbolMap;
    ///
    /// // let data = std::fs::read("c8374b6d-6e96-34d8-ae38-efaa5fec424f.bcsymbolmap").unwrap();
    /// # let data =
    /// #     std::fs::read("tests/fixtures/c8374b6d-6e96-34d8-ae38-efaa5fec424f.bcsymbolmap")
    /// #         .unwrap();
    /// let map = BcSymbolMap::parse(&data).unwrap();
    ///
    /// assert_eq!(map.resolve("__hidden#43_"), "Sources/Sentry/Public/SentryMessage.h");
    /// assert_eq!(map.resolve("_addJSONData"), "_addJSONData");  // #64
    /// ```
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
    pub fn iter(&self) -> BcSymbolMapIterator<'_, 'd> {
        BcSymbolMapIterator {
            iter: self.names.iter(),
        }
    }
}

/// Iterator over the names in a [`BCSymbolMap`].
///
/// This struct is created by [`BCSymbolMap::iter`].
pub struct BcSymbolMapIterator<'a, 'd> {
    iter: std::slice::Iter<'a, &'d str>,
}

impl<'a, 'd> Iterator for BcSymbolMapIterator<'a, 'd> {
    type Item = &'d str;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().copied()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl FusedIterator for BcSymbolMapIterator<'_, '_> {}

/// Error type when parsing an Apple PropertyList into [`BitcodeUuidMapping`].
#[derive(Debug, Error)]
#[error("{kind}")]
pub struct PListError {
    kind: PListErrorKind,
    #[source]
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl PListError {
    /// The kind of error, giving a little more detail.
    pub fn kind(&self) -> PListErrorKind {
        self.kind
    }
}

impl From<elementtree::Error> for PListError {
    fn from(source: elementtree::Error) -> Self {
        Self {
            kind: PListErrorKind::Parse,
            source: Some(Box::new(source)),
        }
    }
}

impl From<PListErrorKind> for PListError {
    fn from(kind: PListErrorKind) -> Self {
        Self { kind, source: None }
    }
}

impl From<ParseDebugIdError> for PListError {
    fn from(source: ParseDebugIdError) -> Self {
        Self {
            kind: PListErrorKind::ParseValue,
            source: Some(Box::new(source)),
        }
    }
}

/// Error kind for [`PListError`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PListErrorKind {
    /// The plist did not have the expected (XML) schema.
    Schema,
    /// There was an (XML) parsing error parsing the plist.
    Parse,
    /// Failed to parse a required PList value.
    ParseValue,
    /// Failed to parse UUID from filename.
    ParseFilename,
}

impl fmt::Display for PListErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Schema => write!(f, "XML structure did not match expected schema"),
            Self::Parse => write!(f, "Invalid XML"),
            Self::ParseValue => write!(f, "Failed to parse a value into the right type"),
            Self::ParseFilename => write!(f, "Failed to parse UUID from filename"),
        }
    }
}

/// A mapping from the `dSYM` UUID to the `BCSymbolMap` UUID.
///
/// When Apple compiles objects from bitcode these objects will have new UUIDs as debug
/// identifiers.  This mapping can be found in the
/// `dSYMs/<object-id>/Contents/Resources/<object-id>.plist` file of downloaded debugging
/// symbols.  This struct allows you to keep track of such a mapping and provides support
/// for parsing it from the ProperyList file format.
#[derive(Clone, Copy, Debug)]
pub struct BitcodeUuidMapping {
    bitcode_uuid: DebugId,
    dsym_uuid: DebugId,
}

impl BitcodeUuidMapping {
    /// Parses a PropertyList containing a `DBGOriginalUUID` mapping.
    ///
    /// The `filename` may contain multiple path segments, the stem of the filename segment
    /// should contain the UUID of the `dSYM`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    /// use symbolic_common::DebugId;
    /// use symbolic_debuginfo::macho::BitcodeUuidMapping;
    ///
    /// let filename = Path::new("2d10c42f-591d-3265-b147-78ba0868073f.plist");
    /// # let filename = Path::new("tests/fixtures/2d10c42f-591d-3265-b147-78ba0868073f.plist");
    /// let dsym_uuid: DebugId = filename
    ///     .file_stem().unwrap()
    ///     .to_str().unwrap()
    ///     .parse().unwrap();
    /// let data = std::fs::read(filename).unwrap();
    ///
    /// let uuid_map = BitcodeUuidMapping::parse_plist(dsym_uuid, &data).unwrap();
    ///
    /// assert_eq!(uuid_map.dsym_uuid(), dsym_uuid);
    /// assert_eq!(
    ///     uuid_map.bitcode_uuid(),
    ///     "c8374b6d-6e96-34d8-ae38-efaa5fec424f".parse().unwrap(),
    /// )
    /// ```
    pub fn parse_plist(dsym_uuid: DebugId, data: &[u8]) -> Result<Self, PListError> {
        Ok(Self {
            bitcode_uuid: uuid_from_plist(data)?,
            dsym_uuid,
        })
    }

    /// Parses a PropertyList containing a `DBGOriginalUUID` mapping.
    ///
    /// This is a convenience version of [`BitcodeUuidMapping::parse_plist`] which extracts
    /// the UUID from the `filename`.
    ///
    /// The `filename` may contain multiple path segments, the stem of the filename segment
    /// should contain the UUID of the `dSYM`.  This is the format the PList is normally
    /// found in a `dSYM` directory structure.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    /// use symbolic_common::DebugId;
    /// use symbolic_debuginfo::macho::BitcodeUuidMapping;
    ///
    /// let filename = Path::new("Contents/Resources/2D10C42F-591D-3265-B147-78BA0868073F.plist");
    /// # let filename = Path::new("tests/fixtures/2d10c42f-591d-3265-b147-78ba0868073f.plist");
    /// let data = std::fs::read(filename).unwrap();
    ///
    /// let uuid_map = BitcodeUuidMapping::parse_plist_with_filename(filename, &data).unwrap();
    ///
    /// assert_eq!(
    ///     uuid_map.dsym_uuid(),
    ///     "2d10c42f-591d-3265-b147-78ba0868073f".parse().unwrap(),
    /// );
    /// assert_eq!(
    ///     uuid_map.bitcode_uuid(),
    ///     "c8374b6d-6e96-34d8-ae38-efaa5fec424f".parse().unwrap(),
    /// )
    /// ```
    pub fn parse_plist_with_filename(filename: &Path, data: &[u8]) -> Result<Self, PListError> {
        let dsym_uuid = filename
            .file_stem()
            .ok_or_else(|| PListError::from(PListErrorKind::ParseFilename))?
            .to_str()
            .ok_or_else(|| PListError::from(PListErrorKind::ParseFilename))?
            .parse()?;
        Self::parse_plist(dsym_uuid, data)
    }

    /// Returns the UUID of the original bitcode.
    pub fn bitcode_uuid(&self) -> DebugId {
        self.bitcode_uuid
    }

    /// Returns the UUID of the compiled binary and associated `dSYM`.
    pub fn dsym_uuid(&self) -> DebugId {
        self.dsym_uuid
    }
}

fn uuid_from_plist(data: &[u8]) -> Result<DebugId, PListError> {
    let plist = Element::from_reader(Cursor::new(data))?;

    let raw = uuid_from_xml_plist(plist).ok_or_else(|| PListError::from(PListErrorKind::Schema))?;

    raw.parse().map_err(Into::into)
}

fn uuid_from_xml_plist(plist: Element) -> Option<String> {
    let version = plist.get_attr("version")?;
    if version != "1.0" {
        return None;
    }
    let dict = plist.find("dict")?;

    let mut found_key = false;
    let mut raw_original = None;
    for element in dict.children() {
        if element.tag().name() == "key" && element.text() == "DBGOriginalUUID" {
            found_key = true;
        } else if found_key {
            raw_original = Some(element.text().to_string());
            break;
        }
    }

    raw_original
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bcsymbolmap_test() {
        let buf = b"BCSymbolMap Vers";
        assert!(BcSymbolMap::test(&buf[..]));

        let buf = b"oops";
        assert!(!BcSymbolMap::test(&buf[..]));
    }

    #[test]
    fn test_basic() {
        let data = std::fs::read_to_string(
            "tests/fixtures/c8374b6d-6e96-34d8-ae38-efaa5fec424f.bcsymbolmap",
        )
        .unwrap();

        assert!(BcSymbolMap::test(&data.as_bytes()[..20]));

        let map = BcSymbolMap::parse(data.as_bytes()).unwrap();
        assert_eq!(map.get(2), Some("-[SentryMessage serialize]"))
    }

    #[test]
    fn test_iter() {
        let data = std::fs::read_to_string(
            "tests/fixtures/c8374b6d-6e96-34d8-ae38-efaa5fec424f.bcsymbolmap",
        )
        .unwrap();
        let map = BcSymbolMap::parse(data.as_bytes()).unwrap();

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
            let map = BcSymbolMap::parse(data.as_bytes()).unwrap();
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
        let map = BcSymbolMap::parse(data.as_bytes()).unwrap();

        assert_eq!(map.resolve("normal_name"), "normal_name");
        assert_eq!(map.resolve("__hidden#2_"), "-[SentryMessage serialize]");
    }

    #[test]
    fn test_plist() {
        let uuid: DebugId = "2d10c42f-591d-3265-b147-78ba0868073f".parse().unwrap();
        let data =
            std::fs::read("tests/fixtures/2d10c42f-591d-3265-b147-78ba0868073f.plist").unwrap();
        let map = BitcodeUuidMapping::parse_plist(uuid, &data).unwrap();

        assert_eq!(map.dsym_uuid(), uuid);
        assert_eq!(
            map.bitcode_uuid(),
            "c8374b6d-6e96-34d8-ae38-efaa5fec424f".parse().unwrap()
        );
    }
}
