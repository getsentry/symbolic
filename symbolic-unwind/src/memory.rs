use super::base::{Address, ReadBytes};
use byteorder::ByteOrder;

/// Provides access to a region of memory.
pub trait MemoryRegion {
    /// This memory region's base address.
    fn base_addr(&self) -> u64;

    /// This memory region's size in bytes.
    fn size(&self) -> usize;

    /// Returns true if this memory region's size is 0.
    fn is_empty(&self) -> bool;

    /// Read the value saved at `address` in this memory region as a value of type `A`.
    ///
    /// The method is generic over the type of address, which doubles as the return type,
    /// as well as `ByteOrder` (i.e. endianness).
    /// Fails if no valid value of type `A` can be read at `address`, e.g. if there are
    /// not enough bytes.
    fn get<A: Address + ReadBytes, E: ByteOrder>(&self, address: A) -> Option<A>;
}

/// A view into a region of memory, given by a slice and a base address.
pub struct MemorySlice<'a> {
    /// The starting address of the memory region.
    base_addr: u64,

    /// The contents of the memory region.
    contents: &'a [u8],
}

impl<'a> MemoryRegion for MemorySlice<'a> {
    fn base_addr(&self) -> u64 {
        self.base_addr
    }

    fn size(&self) -> usize {
        self.contents.len()
    }

    fn is_empty(&self) -> bool {
        self.contents.is_empty()
    }

    fn get<A: Address + ReadBytes, E: ByteOrder>(&self, address: A) -> Option<A> {
        let index = (address.try_into().ok()?).checked_sub(self.base_addr as usize)?;
        A::read_bytes::<E>(self.contents.get(index..)?)
    }
}
