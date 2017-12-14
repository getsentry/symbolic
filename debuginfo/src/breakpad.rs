use std::fmt;

use uuid::Uuid;

use symbolic_common::{Arch, ErrorKind, Result};

/// Unique identifier of a Breakpad code module
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Copy)]
pub struct BreakpadId {
    uuid: Uuid,
    age: u32,
}

impl BreakpadId {
    /// Parses a `BreakpadId` from a 33 character `String`
    pub fn parse(input: &str) -> Result<BreakpadId> {
        if input.len() != 33 {
            return Err(ErrorKind::Parse("Invalid input string length").into());
        }

        let uuid = Uuid::parse_str(&input[..32]).map_err(|_| ErrorKind::Parse("UUID parse error"))?;
        let age = u32::from_str_radix(&input[32..], 16)?;
        Ok(BreakpadId { uuid, age })
    }

    /// Constructs a `BreakpadId` from its `uuid`
    pub fn from_uuid(uuid: Uuid) -> BreakpadId {
        Self::from_parts(uuid, 0)
    }

    /// Constructs a `BreakpadId` from its `uuid` and `age` parts
    pub fn from_parts(uuid: Uuid, age: u32) -> BreakpadId {
        BreakpadId { uuid, age }
    }

    /// Returns the UUID part of the code module's debug_identifier
    pub fn uuid(&self) -> Uuid {
        self.uuid
    }

    /// Returns the age part of the code module's debug identifier
    ///
    /// On Windows, this is an incrementing counter to identify the build.
    /// On all other platforms, this value will always be zero.
    pub fn age(&self) -> u32 {
        self.age
    }
}

impl fmt::Display for BreakpadId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let uuid = self.uuid.simple().to_string().to_uppercase();
        write!(f, "{}{:X}", uuid, self.age)
    }
}

impl Into<String> for BreakpadId {
    fn into(self) -> String {
        self.to_string()
    }
}

#[test]
fn test_parse() {
    assert_eq!(
        BreakpadId::parse("DFB8E43AF2423D73A453AEB6A777EF75A").unwrap(),
        BreakpadId {
            uuid: Uuid::parse_str("DFB8E43AF2423D73A453AEB6A777EF75").unwrap(),
            age: 10,
        }
    );
}

#[test]
fn test_to_string() {
    let id = BreakpadId {
        uuid: Uuid::parse_str("DFB8E43AF2423D73A453AEB6A777EF75").unwrap(),
        age: 10,
    };

    assert_eq!(id.to_string(), "DFB8E43AF2423D73A453AEB6A777EF75A");
}

#[test]
fn test_parse_error() {
    assert!(BreakpadId::parse("DFB8E43AF2423D73A").is_err());
}


/// Provides access to information in a breakpad file
#[derive(Debug)]
pub struct BreakpadSym {
    id: BreakpadId,
    arch: Arch,
}

impl BreakpadSym {
    /// Parses a breakpad file header
    ///
    /// Example:
    /// ```
    /// MODULE mac x86_64 13DA2547B1D53AF99F55ED66AF0C7AF70 Electron Framework
    /// ```
    pub fn parse(bytes: &[u8]) -> Result<BreakpadSym> {
        let mut words = bytes.splitn(5, |b| *b == b' ');

        match words.next() {
            Some(b"MODULE") => (),
            _ => return Err(ErrorKind::BadBreakpadSym("Invalid breakpad magic").into()),
        };

        // Operating system not needed
        words.next();

        let arch = match words.next() {
            Some(word) => String::from_utf8_lossy(word),
            None => return Err(ErrorKind::BadBreakpadSym("Missing breakpad arch").into()),
        };

        let uuid_hex = match words.next() {
            Some(word) => String::from_utf8_lossy(word),
            None => return Err(ErrorKind::BadBreakpadSym("Missing breakpad uuid").into()),
        };

        let id = match BreakpadId::parse(&uuid_hex[0..33]) {
            Ok(uuid) => uuid,
            Err(_) => return Err(ErrorKind::Parse("Invalid breakpad uuid").into()),
        };

        Ok(BreakpadSym {
            id: id,
            arch: Arch::from_breakpad(arch.as_ref())?,
        })
    }

    pub fn id(&self) -> BreakpadId {
        self.id
    }

    pub fn uuid(&self) -> Uuid {
        // TODO: To avoid collisions, this should hash the age in
        self.id().uuid()
    }

    pub fn arch(&self) -> Arch {
        self.arch
    }
}
