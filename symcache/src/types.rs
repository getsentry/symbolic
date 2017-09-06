use symbolic_common::{Result, ErrorKind};

use uuid::Uuid;

#[repr(C, packed)]
#[derive(Default, Copy, Clone)]
pub struct CacheFileHeader {
    pub version: u32,
    pub uuid: Uuid,
    pub arch: [u8; 16],
    pub name_id: u32,
    pub index_start: u32,
    pub index_count: u32,
    pub slices_start: u32,
    pub slices_count: u32,
}

#[repr(C, packed)]
pub struct StoredSlice {
    pub offset: u32,
    pub len: u32,
}

#[repr(C, packed)]
pub struct IndexItem {
    addr_low: u32,
    addr_high: u16,
    filename_id: u16,
    symbol_id: u32,
}

impl IndexItem {
    pub fn addr(&self) -> u64 {
        ((self.addr_high as u64) << 32) | (self.addr_low as u64)
    }
}
