use byteorder::ByteOrder;
use std::convert::TryInto;
use std::ops::{Add, Div, Mul, Rem, Sub};

/// A trait for types that can be read from a slice of bytes.
pub trait ReadBytes: Sized {
    /// The number of bytes that need to be read to produce one value of this type.
    const WIDTH: usize;
    /// Attempt to read a value of this type from a slice of bytes.
    ///
    /// May fail if an invalid byte is encountered or there are not enough bytes in the slice.
    fn read_bytes<B: ByteOrder>(bytes: &[u8]) -> Option<Self>;
}

/// A trait for types that can be used as memory addresses.
///
/// This contains no actual functionality, it only bundles other traits.
pub trait Address:
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
    + std::fmt::Debug
{
}

impl ReadBytes for u8 {
    const WIDTH: usize = 1;
    fn read_bytes<B: ByteOrder>(bytes: &[u8]) -> Option<Self> {
        bytes.first().copied()
    }
}

impl ReadBytes for u16 {
    const WIDTH: usize = 2;
    fn read_bytes<B: ByteOrder>(bytes: &[u8]) -> Option<Self> {
        (bytes.len() >= Self::WIDTH).then(|| B::read_u16(bytes))
    }
}

impl ReadBytes for u32 {
    const WIDTH: usize = 4;
    fn read_bytes<B: ByteOrder>(bytes: &[u8]) -> Option<Self> {
        (bytes.len() >= Self::WIDTH).then(|| B::read_u32(bytes))
    }
}

impl ReadBytes for u64 {
    const WIDTH: usize = 8;
    fn read_bytes<B: ByteOrder>(bytes: &[u8]) -> Option<Self> {
        (bytes.len() >= Self::WIDTH).then(|| B::read_u64(bytes))
    }
}

impl ReadBytes for i8 {
    const WIDTH: usize = 1;
    fn read_bytes<B: ByteOrder>(bytes: &[u8]) -> Option<Self> {
        bytes.first().map(|b| *b as _)
    }
}

impl ReadBytes for i16 {
    const WIDTH: usize = 2;
    fn read_bytes<B: ByteOrder>(bytes: &[u8]) -> Option<Self> {
        (bytes.len() >= Self::WIDTH).then(|| B::read_i16(bytes))
    }
}

impl ReadBytes for i32 {
    const WIDTH: usize = 4;
    fn read_bytes<B: ByteOrder>(bytes: &[u8]) -> Option<Self> {
        (bytes.len() >= Self::WIDTH).then(|| B::read_i32(bytes))
    }
}

impl ReadBytes for i64 {
    const WIDTH: usize = 8;
    fn read_bytes<B: ByteOrder>(bytes: &[u8]) -> Option<Self> {
        (bytes.len() >= Self::WIDTH).then(|| B::read_i64(bytes))
    }
}

impl Address for u8 {}
impl Address for u16 {}
impl Address for u32 {}
impl Address for u64 {}
