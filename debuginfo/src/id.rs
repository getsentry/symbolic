use std::fmt;
use std::str;
use regex::Regex;
use uuid::Uuid;

lazy_static! {
    static ref DEBUG_ID_RE: Regex = Regex::new(r"^(?i)([0-9a-f]{8}-?[0-9a-f]{4}-?[0-9a-f]{4}-?[0-9a-f]{4}-?[0-9a-f]{12})-?([0-9a-f]{1,8})?$").unwrap();
}

#[derive(Debug, Fail, Clone, Copy)]
#[fail(display = "invalid debug id")]
pub struct ParseDebugIdError;

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
/// # extern crate symbolic_debuginfo;
/// use std::str::FromStr;
/// use symbolic_debuginfo::DebugId;
/// # use symbolic_debuginfo::ParseDebugIdError;
///
/// # fn foo() -> Result<(), ParseDebugIdError> {
/// let id = DebugId::from_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75-a")?;
/// assert_eq!("dfb8e43a-f242-3d73-a453-aeb6a777ef75-a".to_string(), id.to_string());
/// # Ok(())
/// # }
///
/// # fn main() { foo().unwrap() }
/// ```
#[repr(C, packed)]
#[derive(Default, Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Copy)]
pub struct DebugId {
    uuid: Uuid,
    appendix: u32,
    _padding: [u8; 12],
}

impl DebugId {
    /// Constructs a `DebugId` from its `uuid`.
    pub fn from_uuid(uuid: Uuid) -> DebugId {
        Self::from_parts(uuid, 0)
    }

    /// Constructs a `DebugId` from its `uuid` and `appendix` parts.
    pub fn from_parts(uuid: Uuid, appendix: u32) -> DebugId {
        DebugId {
            uuid,
            appendix,
            _padding: [0; 12],
        }
    }

    /// Parses a breakpad identifier from a string.
    pub fn from_breakpad(string: &str) -> Result<DebugId, ParseDebugIdError> {
        // Technically, we are are too permissive here by allowing dashes, but
        // we are complete.
        string.parse()
    }

    /// Returns the UUID part of the code module's debug_identifier.
    pub fn uuid(&self) -> Uuid {
        self.uuid
    }

    /// Returns the appendix part of the code module's debug identifier.
    ///
    /// On Windows, this is an incrementing counter to identify the build.
    /// On all other platforms, this value will always be zero.
    pub fn appendix(&self) -> u32 {
        self.appendix
    }

    /// Returns a wrapper which when formatted via `fmt::Display` will format a
    /// a breakpad identifier.
    pub fn breakpad(&self) -> BreakpadFormat {
        BreakpadFormat { inner: self }
    }
}

impl fmt::Debug for DebugId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("DebugId")
            .field("uuid", &self.uuid())
            .field("appendix", &self.appendix())
            .finish()
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
    type Err = ParseDebugIdError;

    fn from_str(string: &str) -> Result<DebugId, ParseDebugIdError> {
        let captures = DEBUG_ID_RE.captures(string).ok_or(ParseDebugIdError)?;
        let uuid = captures
            .get(1)
            .unwrap()
            .as_str()
            .parse()
            .map_err(|_| ParseDebugIdError)?;
        let appendix = captures
            .get(2)
            .map_or(Ok(0), |s| u32::from_str_radix(s.as_str(), 16))
            .map_err(|_| ParseDebugIdError)?;
        Ok(DebugId::from_parts(uuid, appendix))
    }
}

impl From<Uuid> for DebugId {
    fn from(uuid: Uuid) -> DebugId {
        DebugId::from_uuid(uuid)
    }
}

impl From<(Uuid, u32)> for DebugId {
    fn from(tuple: (Uuid, u32)) -> DebugId {
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
/// # extern crate symbolic_debuginfo;
/// use std::str::FromStr;
/// use symbolic_debuginfo::DebugId;
/// # use symbolic_debuginfo::ParseDebugIdError;
///
/// # fn foo() -> Result<(), ParseDebugIdError> {
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
    fn test_parse_compact() {
        assert_eq!(
            DebugId::from_str("dfb8e43af2423d73a453aeb6a777ef75feedface").unwrap(),
            DebugId::from_parts(
                Uuid::parse_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75").unwrap(),
                4277009102,
            )
        );
    }

    #[test]
    fn test_parse_upper() {
        assert_eq!(
            DebugId::from_str("DFB8E43A-F242-3D73-A453-AEB6A777EF75-FEEDFACE").unwrap(),
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
        assert!(DebugId::from_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75-feedface1").is_err())
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
        assert!(DebugId::from_breakpad("DFB8E43AF2423D73A453AEB6A777EF7").is_err());
    }

    #[test]
    fn test_parse_breakpad_error_long() {
        assert!(DebugId::from_breakpad("DFB8E43AF2423D73A453AEB6A777EF75feedface1").is_err())
    }
}
