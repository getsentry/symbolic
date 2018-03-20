use std::fmt;
use std::str;
use uuid::Uuid;
use symbolic_common::{Error, ErrorKind, Result};

/// Unique identifier for debug information files and their debug information.
///
/// The string representation must be between 33 and 40 characters long and
/// consist of:
///
/// 1. 36 character hyphenated hex representation of the UUID field
/// 2. 1-16 character lowercase hex representation of the u64 appendix
///
/// **Example:**
///
/// ```
/// # extern crate symbolic_common;
/// # extern crate symbolic_debuginfo;
/// use std::str::FromStr;
/// # use symbolic_common::Result;
/// use symbolic_debuginfo::DebugId;
///
/// # fn foo() -> Result<()> {
/// let id = DebugId::from_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75-a")?;
/// assert_eq!("dfb8e43a-f242-3d73-a453-aeb6a777ef75-a".to_string(), id.to_string());
/// # Ok(())
/// # }
///
/// # fn main() { foo().unwrap() }
/// ```
#[repr(C, packed)]
#[derive(Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Copy)]
pub struct DebugId {
    uuid: Uuid,
    appendix: u64,
    _padding: u64,
}

impl DebugId {
    /// Constructs a `DebugId` from its `uuid`.
    pub fn from_uuid(uuid: Uuid) -> DebugId {
        Self::from_parts(uuid, 0)
    }

    /// Constructs a `DebugId` from its `uuid` and `appendix` parts.
    pub fn from_parts(uuid: Uuid, appendix: u64) -> DebugId {
        DebugId {
            uuid,
            appendix,
            _padding: 0,
        }
    }

    /// Parses a breakpad identifier from a string.
    pub fn from_breakpad(string: &str) -> Result<DebugId> {
        if string.len() < 33 || string.len() > 40 {
            return Err(ErrorKind::Parse("Invalid input string length").into());
        }

        let uuid =
            Uuid::parse_str(&string[..32]).map_err(|_| ErrorKind::Parse("UUID parse error"))?;
        let appendix = u32::from_str_radix(&string[32..], 16)?;
        Ok(DebugId::from_parts(uuid, appendix as u64))
    }

    /// Returns the UUID part of the code module's debug_identifier.
    pub fn uuid(&self) -> Uuid {
        self.uuid
    }

    /// Returns the appendix part of the code module's debug identifier.
    ///
    /// On Windows, this is an incrementing counter to identify the build.
    /// On all other platforms, this value will always be zero.
    pub fn appendix(&self) -> u64 {
        self.appendix
    }

    /// Returns a wrapper which when formatted via `fmt::Display` will format a
    /// a breakpad identifier.
    pub fn breakpad(&self) -> BreakpadFormat {
        BreakpadFormat { inner: self }
    }
}

impl fmt::Display for DebugId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.uuid.fmt(f)?;
        if self.appendix > 0 {
            write!(f, "-{:x}", { self.appendix })?;
        }
        Ok(())
    }
}

impl str::FromStr for DebugId {
    type Err = Error;

    fn from_str(string: &str) -> Result<DebugId> {
        if string.len() < 36 || string.len() > 53 {
            return Err(ErrorKind::Parse("Invalid input string length").into());
        }

        let uuid =
            Uuid::parse_str(&string[..36]).map_err(|_| ErrorKind::Parse("UUID parse error"))?;
        let appendix = string
            .get(37..)
            .map_or(Ok(0), |s| u64::from_str_radix(s, 16))?;
        Ok(DebugId::from_parts(uuid, appendix))
    }
}

impl From<Uuid> for DebugId {
    fn from(uuid: Uuid) -> DebugId {
        DebugId::from_uuid(uuid)
    }
}

impl From<(Uuid, u64)> for DebugId {
    fn from(tuple: (Uuid, u64)) -> DebugId {
        let (uuid, appendix) = tuple;
        DebugId::from_parts(uuid, appendix)
    }
}

#[cfg(feature = "with_serde")]
derive_deserialize_from_str!(DebugId, "DebugId");

#[cfg(feature = "with_serde")]
derive_serialize_from_display!(DebugId);

/// Wrapper around `DebugId` for Breakpad formatting.
///
/// **Example:**
///
/// ```
/// # extern crate symbolic_common;
/// # extern crate symbolic_debuginfo;
/// use std::str::FromStr;
/// # use symbolic_common::Result;
/// use symbolic_debuginfo::DebugId;
///
/// # fn foo() -> Result<()> {
/// let id = DebugId::from_breakpad("DFB8E43AF2423D73A453AEB6A777EF75a")?;
/// assert_eq!("DFB8E43AF2423D73A453AEB6A777EF75a".to_string(), id.breakpad().to_string());
/// # Ok(())
/// # }
///
/// # fn main() { foo().unwrap() }
/// ```
#[derive(Debug)]
pub struct BreakpadFormat<'a> {
    inner: &'a DebugId,
}

impl<'a> fmt::Display for BreakpadFormat<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:X}{:x}",
            self.inner.uuid().simple(),
            self.inner.appendix()
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_parse_zero() {
        assert_eq!(
            DebugId::from_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75").unwrap(),
            DebugId::from_parts(
                Uuid::parse_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75").unwrap(),
                0,
            )
        );
    }

    #[test]
    fn test_parse_short() {
        assert_eq!(
            DebugId::from_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75-a").unwrap(),
            DebugId::from_parts(
                Uuid::parse_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75").unwrap(),
                10,
            )
        );
    }

    #[test]
    fn test_parse_long() {
        assert_eq!(
            DebugId::from_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75-feedface").unwrap(),
            DebugId::from_parts(
                Uuid::parse_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75").unwrap(),
                4277009102,
            )
        );
    }

    #[test]
    fn test_to_string_zero() {
        let id = DebugId::from_parts(
            Uuid::parse_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75").unwrap(),
            0,
        );

        assert_eq!(id.to_string(), "dfb8e43a-f242-3d73-a453-aeb6a777ef75");
    }

    #[test]
    fn test_to_string_short() {
        let id = DebugId::from_parts(
            Uuid::parse_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75").unwrap(),
            10,
        );

        assert_eq!(id.to_string(), "dfb8e43a-f242-3d73-a453-aeb6a777ef75-a");
    }

    #[test]
    fn test_to_string_long() {
        let id = DebugId::from_parts(
            Uuid::parse_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75").unwrap(),
            4277009102,
        );

        assert_eq!(
            id.to_string(),
            "dfb8e43a-f242-3d73-a453-aeb6a777ef75-feedface"
        );
    }

    #[test]
    fn test_parse_error_short() {
        assert!(DebugId::from_str("dfb8e43a-f242-3d73-a453-aeb6a777ef7").is_err());
    }

    #[test]
    fn test_parse_error_long() {
        assert!(
            DebugId::from_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75-feedfacefeedface1").is_err()
        )
    }

    #[test]
    fn test_parse_breakpad_zero() {
        assert_eq!(
            DebugId::from_breakpad("DFB8E43AF2423D73A453AEB6A777EF750").unwrap(),
            DebugId::from_parts(
                Uuid::parse_str("DFB8E43AF2423D73A453AEB6A777EF75").unwrap(),
                0,
            )
        );
    }

    #[test]
    fn test_parse_breakpad_short() {
        assert_eq!(
            DebugId::from_breakpad("DFB8E43AF2423D73A453AEB6A777EF75a").unwrap(),
            DebugId::from_parts(
                Uuid::parse_str("DFB8E43AF2423D73A453AEB6A777EF75").unwrap(),
                10,
            )
        );
    }

    #[test]
    fn test_parse_breakpad_long() {
        assert_eq!(
            DebugId::from_breakpad("DFB8E43AF2423D73A453AEB6A777EF75feedface").unwrap(),
            DebugId::from_parts(
                Uuid::parse_str("DFB8E43AF2423D73A453AEB6A777EF75").unwrap(),
                4277009102,
            )
        );
    }

    #[test]
    fn test_to_string_breakpad_zero() {
        let id = DebugId::from_parts(
            Uuid::parse_str("DFB8E43AF2423D73A453AEB6A777EF75").unwrap(),
            0,
        );

        assert_eq!(
            id.breakpad().to_string(),
            "DFB8E43AF2423D73A453AEB6A777EF750"
        );
    }

    #[test]
    fn test_to_string_breakpad_short() {
        let id = DebugId::from_parts(
            Uuid::parse_str("DFB8E43AF2423D73A453AEB6A777EF75").unwrap(),
            10,
        );

        assert_eq!(
            id.breakpad().to_string(),
            "DFB8E43AF2423D73A453AEB6A777EF75a"
        );
    }

    #[test]
    fn test_to_string_breakpad_long() {
        let id = DebugId::from_parts(
            Uuid::parse_str("DFB8E43AF2423D73A453AEB6A777EF75").unwrap(),
            4277009102,
        );

        assert_eq!(
            id.breakpad().to_string(),
            "DFB8E43AF2423D73A453AEB6A777EF75feedface"
        );
    }

    #[test]
    fn test_parse_breakpad_error_short() {
        assert!(DebugId::from_breakpad("DFB8E43AF2423D73A453AEB6A777EF75").is_err());
    }

    #[test]
    fn test_parse_breakpad_error_long() {
        assert!(DebugId::from_breakpad("DFB8E43AF2423D73A453AEB6A777EF75feedface1").is_err())
    }
}
