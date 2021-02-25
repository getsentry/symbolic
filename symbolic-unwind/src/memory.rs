/// Provides access to a region of memory.
pub trait MemoryRegion<U> {
    /// This memory region's base address.
    fn base_addr(&self) -> u64;

    /// This memory region's size in bytes.
    fn size(&self) -> u32;

    /// Read the value saved at `address` in this memory region as a value of size `U`.
    ///
    /// Fails if the value would exceed the region's boundary.
    fn get(&self, address: u64) -> Option<U>;
}

/// A view into a region of memory, given by a slice and a base address.
pub struct MemorySlice<'a> {
    /// The starting address of the memory region.
    base_addr: u64,

    /// The contents of the memory region.
    ///
    /// This may be at most [`std::u32::MAX`] elements long.
    contents: &'a [u8],
}

impl<'a> MemorySlice<'a> {
    /// Creates a new `MemorySlice` from a base address and a slice.
    ///
    /// This fails if the length of the slice is greater than [`std::u32::MAX`].
    pub fn new(base_addr: u64, contents: &'a [u8]) -> Option<Self> {
        (contents.len() <= std::u32::MAX as usize).then(|| Self {
            base_addr,
            contents,
        })
    }
}

impl<'a> MemoryRegion<u8> for MemorySlice<'a> {
    fn base_addr(&self) -> u64 {
        self.base_addr
    }

    fn size(&self) -> u32 {
        self.contents.len() as u32
    }

    fn get(&self, address: u64) -> Option<u8> {
        self.contents.get(address as usize).copied()
    }
}

impl<'a> MemoryRegion<u16> for MemorySlice<'a> {
    fn base_addr(&self) -> u64 {
        self.base_addr
    }

    fn size(&self) -> u32 {
        self.contents.len() as u32
    }

    fn get(&self, address: u64) -> Option<u16> {
        let b1 = self.contents.get(address as usize).copied()?;
        let b2 = self.contents.get(address as usize + 1).copied()?;
        Some((b1 as u16) << 8 | b2 as u16)
    }
}

impl<'a> MemoryRegion<u32> for MemorySlice<'a> {
    fn base_addr(&self) -> u64 {
        self.base_addr
    }

    fn size(&self) -> u32 {
        self.contents.len() as u32
    }

    fn get(&self, address: u64) -> Option<u32> {
        let b12: u16 = self.get(address)?;
        let b34: u16 = self.get(address + 2)?;
        Some((b12 as u32) << 16 | b34 as u32)
    }
}

impl<'a> MemoryRegion<u64> for MemorySlice<'a> {
    fn base_addr(&self) -> u64 {
        self.base_addr
    }

    fn size(&self) -> u32 {
        self.contents.len() as u32
    }

    fn get(&self, address: u64) -> Option<u64> {
        let b1234: u32 = self.get(address)?;
        let b5678: u32 = self.get(address + 4)?;
        Some((b1234 as u64) << 32 | b5678 as u64)
    }
}

impl<'a> MemoryRegion<i8> for MemorySlice<'a> {
    fn base_addr(&self) -> u64 {
        self.base_addr
    }

    fn size(&self) -> u32 {
        self.contents.len() as u32
    }

    fn get(&self, address: u64) -> Option<i8> {
        self.contents
            .get(address as usize)
            .copied()
            .map(|b| b as i8)
    }
}

impl<'a> MemoryRegion<i16> for MemorySlice<'a> {
    fn base_addr(&self) -> u64 {
        self.base_addr
    }

    fn size(&self) -> u32 {
        self.contents.len() as u32
    }

    fn get(&self, address: u64) -> Option<i16> {
        let b1 = self.contents.get(address as usize).copied()?;
        let b2 = self.contents.get(address as usize + 1).copied()?;
        Some((b1 as i16) << 8 | b2 as i16)
    }
}

impl<'a> MemoryRegion<i32> for MemorySlice<'a> {
    fn base_addr(&self) -> u64 {
        self.base_addr
    }

    fn size(&self) -> u32 {
        self.contents.len() as u32
    }

    fn get(&self, address: u64) -> Option<i32> {
        let b12: i16 = self.get(address)?;
        let b34: i16 = self.get(address + 2)?;
        Some((b12 as i32) << 16 | b34 as i32)
    }
}

impl<'a> MemoryRegion<i64> for MemorySlice<'a> {
    fn base_addr(&self) -> u64 {
        self.base_addr
    }

    fn size(&self) -> u32 {
        self.contents.len() as u32
    }

    fn get(&self, address: u64) -> Option<i64> {
        let b1234: i32 = self.get(address)?;
        let b5678: i32 = self.get(address + 4)?;
        Some((b1234 as i64) << 32 | b5678 as i64)
    }
}
