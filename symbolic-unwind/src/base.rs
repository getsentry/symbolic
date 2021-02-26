use std::convert::TryInto;
use std::fmt::Debug;
use std::ops::{Add, Div, Mul, Rem, Sub};

/// Trait that abstracts over the [endianness](https://en.wikipedia.org/wiki/Endianness)
/// of data representation.
///
/// This trait provides no other functionality than a method for testing whether
/// an endianness is big or little. In particular it does not provide methods for
/// reading number types the way that similar traits/types in `byteorder` and `gimli` do.
pub trait Endianness: Debug + Default + Clone + Copy + PartialEq + Eq {
    /// Returns true if this is big-endian (i.e. most significant bytes first).
    fn is_big_endian(self) -> bool;
}

/// Big-endian data representation (i.e. most significant bits first),
/// known at compile time.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BigEndian;

impl Endianness for BigEndian {
    fn is_big_endian(self) -> bool {
        true
    }
}

/// Little-endian data representation (i.e. least significant bits first),
/// known at compile time.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LittleEndian;

impl Endianness for LittleEndian {
    fn is_big_endian(self) -> bool {
        false
    }
}

/// Endianness that can be selected at run time.
///
/// Defaults to the endianness of the target platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeEndian {
    /// Big-endian data representation.
    Big,
    /// Little-endian data representation.
    Little,
}

impl Default for RuntimeEndian {
    #[cfg(target_endian = "little")]
    fn default() -> Self {
        Self::Little
    }

    #[cfg(target_endian = "big")]
    fn default() -> Self {
        Self::Big
    }
}

impl Endianness for RuntimeEndian {
    fn is_big_endian(self) -> bool {
        self == Self::Big
    }
}

/// The endianness of the target platform, in this case [`BigEndian`].
#[cfg(target_endian = "big")]
pub type NativeEndian = BigEndian;

#[cfg(target_endian = "big")]
#[allow(non_upper_case_globals)]
#[doc(hidden)]
pub const NativeEndian: NativeEndian = BigEndian;

/// The endianness of the target platform, in this case [`LittleEndian`].
#[cfg(target_endian = "little")]
pub type NativeEndian = LittleEndian;

#[cfg(target_endian = "little")]
#[allow(non_upper_case_globals)]
#[doc(hidden)]
pub const NativeEndian: NativeEndian = LittleEndian;

/// A trait for types that can be used as memory addresses.
///
/// This contains no actual functionality, it only bundles other traits.
pub trait RegisterValue:
    TryInto<usize>
      // Not super happy about this; this is mostly so that we can add 1 to addresses.
      // An alternative might be to have an associated constant ONE.
    + From<u8>
    + Add<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Sub<Output = Self>
    + Rem<Output = Self>
    + Copy
    + Sized
    + Debug
{
    /// The number of bytes that need to be read to produce one value of this type.
    const WIDTH: usize;
    /// Attempt to read a value of this type from a slice of bytes.
    ///
    /// May fail if an invalid byte is encountered or there are not enough bytes in the slice.
    fn read_bytes<E: Endianness>(bytes: &[u8], endian: E) -> Option<Self>;
}

impl RegisterValue for u8 {
    const WIDTH: usize = 1;
    fn read_bytes<E: Endianness>(bytes: &[u8], _endian: E) -> Option<Self> {
        bytes.first().copied()
    }
}

impl RegisterValue for u16 {
    const WIDTH: usize = 2;
    fn read_bytes<E: Endianness>(bytes: &[u8], endian: E) -> Option<Self> {
        let bytes: &[u8; Self::WIDTH] = bytes[..Self::WIDTH].try_into().ok()?;
        if endian.is_big_endian() {
            Some(Self::from_be_bytes(*bytes))
        } else {
            Some(Self::from_le_bytes(*bytes))
        }
    }
}

impl RegisterValue for u32 {
    const WIDTH: usize = 4;
    fn read_bytes<E: Endianness>(bytes: &[u8], endian: E) -> Option<Self> {
        let bytes: &[u8; Self::WIDTH] = bytes[..Self::WIDTH].try_into().ok()?;
        if endian.is_big_endian() {
            Some(Self::from_be_bytes(*bytes))
        } else {
            Some(Self::from_le_bytes(*bytes))
        }
    }
}

impl RegisterValue for u64 {
    const WIDTH: usize = 8;
    fn read_bytes<E: Endianness>(bytes: &[u8], endian: E) -> Option<Self> {
        let bytes: &[u8; Self::WIDTH] = bytes[..Self::WIDTH].try_into().ok()?;
        if endian.is_big_endian() {
            Some(Self::from_be_bytes(*bytes))
        } else {
            Some(Self::from_le_bytes(*bytes))
        }
    }
}

//impl RegisterValue for i8 {
//    const WIDTH: usize = 1;
//    fn read_bytes<E: Endianness>(bytes: &[u8], _endian: E) -> Option<Self> {
//        bytes.first().map(|b| *b as _)
//    }
//}
//
//impl RegisterValue for i16 {
//    const WIDTH: usize = 2;
//    fn read_bytes<E: Endianness>(bytes: &[u8], endian: E) -> Option<Self> {
//        let first = u8::read_bytes(bytes[0..1], endian)?;
//        let second = u8::read_bytes(bytes[1..2], endian)?;
//
//        let (top, bot) = if endian.is_big_endian() {
//            (first, second)
//        } else {
//            (second, first)
//        };
//        Some((top as i16) << 8 | bot as i16)
//    }
//}
//
//impl RegisterValue for i32 {
//    const WIDTH: usize = 4;
//    fn read_bytes<E: Endianness>(bytes: &[u8], endian: E) -> Option<Self> {
//        let first = u16::read_bytes(bytes[0..2], endian)?;
//        let second = u16::read_bytes(bytes[2..4], endian)?;
//
//        let (top, bot) = if endian.is_big_endian() {
//            (first, second)
//        } else {
//            (second, first)
//        };
//        Some((top as i32) << 16 | bot as i32)
//    }
//}
//
//impl RegisterValue for i64 {
//    const WIDTH: usize = 8;
//    fn read_bytes<E: Endianness>(bytes: &[u8], endian: E) -> Option<Self> {
//        let first = u32::read_bytes(bytes[0..4], endian)?;
//        let second = u32::read_bytes(bytes[4..8], endian)?;
//
//        let (top, bot) = if endian.is_big_endian() {
//            (first, second)
//        } else {
//            (second, first)
//        };
//        Some((top as i64) << 32 | bot as i64)
//    }
//}

