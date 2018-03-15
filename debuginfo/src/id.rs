use std::fmt;
use std::str;
use uuid::Uuid;
use symbolic_common::{Error, ErrorKind, Result};

/// Unique identifier for `Object` files and their debug information.
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
/// use symbolic_debuginfo::ObjectId;
///
/// # fn foo() -> Result<()> {
/// let id = ObjectId::from_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75-a")?;
/// assert_eq!("dfb8e43a-f242-3d73-a453-aeb6a777ef75-a".to_string(), id.to_string());
/// # Ok(())
/// # }
///
/// # fn main() { foo().unwrap() }
/// ```
#[repr(C, packed)]
#[derive(Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Copy)]
pub struct ObjectId {
    uuid: Uuid,
    appendix: u64,
    _padding: u64,
}

impl ObjectId {
    /// Constructs a `ObjectId` from its `uuid`.
    pub fn from_uuid(uuid: Uuid) -> ObjectId {
        Self::from_parts(uuid, 0)
    }

    /// Constructs a `ObjectId` from its `uuid` and `appendix` parts.
    pub fn from_parts(uuid: Uuid, appendix: u64) -> ObjectId {
        ObjectId {
            uuid,
            appendix,
            _padding: 0,
        }
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
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.uuid.fmt(f)?;
        if self.appendix > 0 {
            write!(f, "-{:x}", { self.appendix })?;
        }
        Ok(())
    }
}

impl str::FromStr for ObjectId {
    type Err = Error;

    fn from_str(string: &str) -> Result<ObjectId> {
        if string.len() < 36 || string.len() > 53 {
            return Err(ErrorKind::Parse("Invalid input string length").into());
        }

        let uuid =
            Uuid::parse_str(&string[..36]).map_err(|_| ErrorKind::Parse("UUID parse error"))?;
        let appendix = string
            .get(37..)
            .map_or(Ok(0), |s| u64::from_str_radix(s, 16))?;
        Ok(ObjectId::from_parts(uuid, appendix))
    }
}

impl From<Uuid> for ObjectId {
    fn from(uuid: Uuid) -> ObjectId {
        ObjectId::from_uuid(uuid)
    }
}

impl From<(Uuid, u64)> for ObjectId {
    fn from(tuple: (Uuid, u64)) -> ObjectId {
        let (uuid, appendix) = tuple;
        ObjectId::from_parts(uuid, appendix)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_parse_zero() {
        assert_eq!(
            ObjectId::from_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75").unwrap(),
            ObjectId::from_parts(
                Uuid::parse_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75").unwrap(),
                0,
            )
        );
    }

    #[test]
    fn test_parse_short() {
        assert_eq!(
            ObjectId::from_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75-a").unwrap(),
            ObjectId::from_parts(
                Uuid::parse_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75").unwrap(),
                10,
            )
        );
    }

    #[test]
    fn test_parse_long() {
        assert_eq!(
            ObjectId::from_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75-feedface").unwrap(),
            ObjectId::from_parts(
                Uuid::parse_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75").unwrap(),
                4277009102,
            )
        );
    }

    #[test]
    fn test_to_string_zero() {
        let id = ObjectId::from_parts(
            Uuid::parse_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75").unwrap(),
            0,
        );

        assert_eq!(id.to_string(), "dfb8e43a-f242-3d73-a453-aeb6a777ef75");
    }

    #[test]
    fn test_to_string_short() {
        let id = ObjectId::from_parts(
            Uuid::parse_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75").unwrap(),
            10,
        );

        assert_eq!(id.to_string(), "dfb8e43a-f242-3d73-a453-aeb6a777ef75-a");
    }

    #[test]
    fn test_to_string_long() {
        let id = ObjectId::from_parts(
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
        assert!(ObjectId::from_str("dfb8e43a-f242-3d73-a453-aeb6a777ef7").is_err());
    }

    #[test]
    fn test_parse_error_long() {
        assert!(
            ObjectId::from_str("dfb8e43a-f242-3d73-a453-aeb6a777ef75-feedfacefeedface1").is_err()
        )
    }
}
