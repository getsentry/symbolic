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
