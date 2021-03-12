//! Basic definitions necessary for stack unwinding.
use std::convert::TryInto;
use std::fmt::Debug;
use std::ops::{Add, Div, Mul, Rem, Sub};
use std::str::FromStr;

/// Trait that abstracts over the [endianness](https://en.wikipedia.org/wiki/Endianness)
/// of data representation.
///
/// This trait provides no other functionality than a method for testing whether
/// an endianness is big or little. In particular it does not provide methods for
/// reading number types the way that similar traits/types in `byteorder` and `gimli` do.
///
/// We use our own trait here so we don't need to depend on crates (`byteorder`/`gimli`)
/// for a minuscule part of their features.
pub trait Endianness: Debug + Clone + Copy {
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
#[repr(C)]
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

/// A trait for types that can be used as memory addresses.
///
/// This contains no actual functionality, it only bundles other traits.
pub trait RegisterValue:
    TryInto<usize>
    + Add<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Sub<Output = Self>
    + Rem<Output = Self>
    + FromStr
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
        let bytes: &[u8; Self::WIDTH] = bytes.get(..Self::WIDTH)?.try_into().ok()?;
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
        let bytes: &[u8; Self::WIDTH] = bytes.get(..Self::WIDTH)?.try_into().ok()?;
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
        let bytes: &[u8; Self::WIDTH] = bytes.get(..Self::WIDTH)?.try_into().ok()?;
        if endian.is_big_endian() {
            Some(Self::from_be_bytes(*bytes))
        } else {
            Some(Self::from_le_bytes(*bytes))
        }
    }
}

impl RegisterValue for i32 {
    const WIDTH: usize = 4;
    fn read_bytes<E: Endianness>(bytes: &[u8], endian: E) -> Option<Self> {
        let bytes: &[u8; Self::WIDTH] = bytes.get(..Self::WIDTH)?.try_into().ok()?;
        if endian.is_big_endian() {
            Some(Self::from_be_bytes(*bytes))
        } else {
            Some(Self::from_le_bytes(*bytes))
        }
    }
}

impl RegisterValue for i64 {
    const WIDTH: usize = 8;
    fn read_bytes<E: Endianness>(bytes: &[u8], endian: E) -> Option<Self> {
        let bytes: &[u8; Self::WIDTH] = bytes.get(..Self::WIDTH)?.try_into().ok()?;
        if endian.is_big_endian() {
            Some(Self::from_be_bytes(*bytes))
        } else {
            Some(Self::from_le_bytes(*bytes))
        }
    }
}

/// A view into a region of memory, given by a slice and a base address.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct MemoryRegion<'a> {
    /// The starting address of the memory region.
    pub base_addr: u64,

    /// The contents of the memory region.
    pub contents: &'a [u8],
}

impl<'a> MemoryRegion<'a> {
    /// This memory region's base address.
    pub fn base_addr(&self) -> u64 {
        self.base_addr
    }

    /// This memory region's length in bytes.
    pub fn len(&self) -> usize {
        self.contents.len()
    }

    /// Returns true if this memory region's size is 0.
    pub fn is_empty(&self) -> bool {
        self.contents.is_empty()
    }

    /// Read the value saved at `address` in this memory region as a value of type `A`.
    ///
    /// The method is generic over the type of address, which doubles as the return type,
    /// as well as `Endianness`.
    /// Fails if no valid value of type `A` can be read at `address`, e.g. if there are
    /// not enough bytes.
    pub fn get<A: RegisterValue, E: Endianness>(&self, address: A, endian: E) -> Option<A> {
        let index = (address.try_into().ok()?).checked_sub(self.base_addr as usize)?;
        A::read_bytes(self.contents.get(index..)?, endian)
    }
}
