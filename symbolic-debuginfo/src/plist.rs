//! Apple PropertyList support.
//!
//! For MachO objects

use std::collections::HashMap;
use std::error::Error as StdError;
use std::fmt;
use std::io::Cursor;

use elementtree::Element;
use thiserror::Error;

use symbolic_common::{DebugId, ParseDebugIdError};

/// The error type for handling a [`PList`].
#[derive(Debug, Error)]
#[error("{kind}")]
pub struct PListError {
    kind: PListErrorKind,
    #[source]
    source: Option<Box<dyn StdError + Send + Sync + 'static>>,
}

impl From<elementtree::Error> for PListError {
    fn from(source: elementtree::Error) -> Self {
        Self {
            kind: PListErrorKind::Xml,
            source: Some(Box::new(source)),
        }
    }
}

impl From<ParseDebugIdError> for PListError {
    fn from(source: ParseDebugIdError) -> Self {
        Self {
            kind: PListErrorKind::Parse,
            source: Some(Box::new(source)),
        }
    }
}

/// Error kind for [`PListError`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PListErrorKind {
    /// The plist did not have the expected XML schema.
    Schema,
    /// There was an XML parsing error.
    Xml,
    /// Failed to parse a PList value.
    Parse,
}

impl fmt::Display for PListErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Schema => write!(f, "XML structure did not match expected schema"),
            Self::Xml => write!(f, "Invalid XML"),
            Self::Parse => write!(f, "Failed to parse a value into the right type"),
        }
    }
}

impl From<PListErrorKind> for PListError {
    fn from(source: PListErrorKind) -> Self {
        Self {
            kind: source,
            source: None,
        }
    }
}

/// Apple PropertyList structure.
///
/// This is an auxiliary file, not an object file.
///
/// The PList format is used to map the UUID of a dSYM compiled from bitcode to the original
/// UUID of the matching [`BCSymbolMap`].
#[derive(Clone, Debug)]
pub struct PList {
    id: DebugId,
    map: HashMap<String, String>,
}

impl PList {
    /// Tests whether the buffer could contain a [`PList`].
    pub fn test(bytes: &[u8]) -> bool {
        bytes.starts_with(b"<?xml")
    }

    /// Parse the plist file, creating a new in-memory representation of it.
    pub fn parse(id: DebugId, data: &[u8]) -> Result<Self, PListError> {
        let mut map = HashMap::new();
        let plist = Element::from_reader(Cursor::new(data))?;

        {
            let version = plist
                .get_attr("version")
                .ok_or(PListError::from(PListErrorKind::Schema))?;
            if version != "1.0" {
                return Err(PListError::from(PListErrorKind::Schema));
            }
        }
        let dict = plist
            .find("dict")
            .ok_or(PListError::from(PListErrorKind::Schema))?;

        let mut last_key = None;
        for element in dict.children() {
            match last_key {
                None => {
                    last_key = Some(element.text().to_string());
                }
                Some(key) => {
                    let value = element.text().to_string();
                    map.insert(key, value);
                    last_key = None;
                }
            }
        }

        Ok(Self { id, map })
    }

    /// Returns whether the plist contains a BCSymbolMap UUID mapping.
    pub fn is_bcsymbol_mapping(&self) -> bool {
        self.map.contains_key("DBGOriginalUUID")
    }

    /// Returns the original UUID if this PList contains a BCSymbolMap UUID mapping.
    pub fn original_uuid(&self) -> Result<Option<DebugId>, PListError> {
        let res = match self.map.get("DBGOriginalUUID") {
            Some(val) => Some(val.parse()?),
            None => None,
        };
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic() {
        let uuid: DebugId = "2d10c42f-591d-3265-b147-78ba0868073f".parse().unwrap();
        let data =
            std::fs::read_to_string("tests/fixtures/2d10c42f-591d-3265-b147-78ba0868073f.plist")
                .unwrap();
        assert!(PList::test(&data.as_bytes()[..8]));

        let plist = PList::parse(uuid, data.as_bytes()).unwrap();
        assert!(plist.is_bcsymbol_mapping());
    }
}
