use std::fmt;
use uuid::Uuid;
use symbolic_common::{ErrorKind, Result};

/// Unique identifier for `Object` files and their debug information.
#[derive(Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Copy)]
pub struct ObjectId {
    uuid: Uuid,
    age: u32,
}

impl ObjectId {
    /// Parses an `ObjectId` from a formatted `String`.
    ///
    /// The string must be between 33 and 40 characters long and consist of:
    /// 1. A 32 character uppercase hex representation of the UUID field
    /// 2. A 1-8 character lowercase hex representation of the u32 age field
    pub fn parse(input: &str) -> Result<ObjectId> {
        if input.len() < 33 || input.len() > 40 {
            return Err(ErrorKind::Parse("Invalid input string length").into());
        }

        let uuid = Uuid::parse_str(&input[..32]).map_err(|_| ErrorKind::Parse("UUID parse error"))?;
        let age = u32::from_str_radix(&input[32..], 16)?;
        Ok(ObjectId { uuid, age })
    }

    /// Constructs a `ObjectId` from its `uuid`.
    pub fn from_uuid(uuid: Uuid) -> ObjectId {
        Self::from_parts(uuid, 0)
    }

    /// Constructs a `ObjectId` from its `uuid` and `age` parts.
    pub fn from_parts(uuid: Uuid, age: u32) -> ObjectId {
        ObjectId { uuid, age }
    }

    /// Returns the UUID part of the code module's debug_identifier.
    pub fn uuid(&self) -> Uuid {
        self.uuid
    }

    /// Returns the age part of the code module's debug identifier.
    ///
    /// On Windows, this is an incrementing counter to identify the build.
    /// On all other platforms, this value will always be zero.
    pub fn age(&self) -> u32 {
        self.age
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let uuid = self.uuid.simple().to_string().to_uppercase();
        write!(f, "{}{:x}", uuid, self.age)
    }
}

impl From<Uuid> for ObjectId {
    fn from(uuid: Uuid) -> ObjectId {
        ObjectId::from_uuid(uuid)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_short() {
        assert_eq!(
            ObjectId::parse("DFB8E43AF2423D73A453AEB6A777EF75a").unwrap(),
            ObjectId {
                uuid: Uuid::parse_str("DFB8E43AF2423D73A453AEB6A777EF75").unwrap(),
                age: 10,
            }
        );
    }

    #[test]
    fn test_parse_long() {
        assert_eq!(
            ObjectId::parse("DFB8E43AF2423D73A453AEB6A777EF75feedface").unwrap(),
            ObjectId {
                uuid: Uuid::parse_str("DFB8E43AF2423D73A453AEB6A777EF75").unwrap(),
                age: 4277009102,
            }
        );
    }

    #[test]
    fn test_to_string_short() {
        let id = ObjectId {
            uuid: Uuid::parse_str("DFB8E43AF2423D73A453AEB6A777EF75").unwrap(),
            age: 10,
        };

        assert_eq!(id.to_string(), "DFB8E43AF2423D73A453AEB6A777EF75a");
    }

    #[test]
    fn test_to_string_long() {
        let id = ObjectId {
            uuid: Uuid::parse_str("DFB8E43AF2423D73A453AEB6A777EF75").unwrap(),
            age: 4277009102,
        };

        assert_eq!(id.to_string(), "DFB8E43AF2423D73A453AEB6A777EF75feedface");
    }

    #[test]
    fn test_parse_error_short() {
        assert!(ObjectId::parse("DFB8E43AF2423D73A453AEB6A777EF75").is_err());
    }

    #[test]
    fn test_parse_error_long() {
        assert!(ObjectId::parse("DFB8E43AF2423D73A453AEB6A777EF75feedface1").is_err())
    }
}
