/// Provides access to a region of memory.
pub trait MemoryRegion {
    /// This memory region's base address.
    fn base_addr(&self) -> u64;

    /// This memory region's size in bytes.
    fn size(&self) -> u32;

    /// Read the value saved at `address` in this memory region as a `u8`.
    ///
    /// Fails if the value would exceed the region's boundary.
    fn get_u8(&self, address: u64) -> Option<u8>;

    /// Read the value saved at `address` in this memory region as a `u16`.
    ///
    /// Fails if the value would exceed the region's boundary.
    fn get_u16(&self, address: u64) -> Option<u16>;

    /// Read the value saved at `address` in this memory region as a `u32`.
    ///
    /// Fails if the value would exceed the region's boundary.
    fn get_u32(&self, address: u64) -> Option<u32>;

    /// Read the value saved at `address` in this memory region as a `u64`.
    ///
    /// Fails if the value would exceed the region's boundary.
    fn get_u64(&self, address: u64) -> Option<u64>;
}
