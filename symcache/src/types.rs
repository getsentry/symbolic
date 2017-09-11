use std::marker::PhantomData;

use uuid::Uuid;


#[repr(C, packed)]
#[derive(Default, Copy, Clone)]
pub struct Seg<T> {
    pub offset: u32,
    pub len: u32,
    _ty: PhantomData<T>,
}

#[repr(C, packed)]
#[derive(Default, Copy, Clone)]
pub struct FileRecord {
    pub filename: Seg<u8>,
    pub comp_dir: Seg<u8>,
}

#[repr(C, packed)]
#[derive(Default, Copy, Clone)]
pub struct FuncRecord {
    pub addr_low: u32,
    pub addr_high: u16,
    pub len: u16,
    pub symbol_id: u32,
    pub parent_id: u32,
    pub line_record_id: u32,
    pub line_start: u32,
}

#[repr(C, packed)]
#[derive(Default, Copy, Clone)]
pub struct LineRecord {
    /// offset to function item or line record
    pub addr_off: u16,
    /// absolutely indexed file
    pub file_id: u16,
    /// offset to previous item or line record
    pub line: u8,
}

#[repr(C, packed)]
#[derive(Default, Copy, Clone)]
pub struct CacheFileHeader {
    pub version: u32,
    pub uuid: Uuid,
    pub arch: [i8; 16],
    pub name_id: u32,
    pub symbols: Seg<Seg<u8>>,
    pub files: Seg<FileRecord>,
    pub function_index: Seg<FuncRecord>,
    pub line_records: Seg<Seg<LineRecord>>,
}

impl FuncRecord {
    pub fn addr_start(&self) -> u64 {
        ((self.addr_high as u64) << 32) | self.addr_low as u64
    }

    pub fn addr_end(&self) -> u64 {
        self.addr_start() + self.len as u64
    }

    pub fn addr_in_range(&self, addr: u64) -> bool {
        addr >= self.addr_start() && addr <= self.addr_end()
    }

    pub fn get_parent_func(&self) -> Option<usize> {
        if self.parent_id == !0 {
            None
        } else {
            Some(self.parent_id as usize)
        }
    }
}
