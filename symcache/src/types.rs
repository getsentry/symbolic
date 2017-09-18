use std::marker::PhantomData;

use uuid::Uuid;


#[repr(C, packed)]
#[derive(Eq, PartialEq, Ord, PartialOrd, Hash, Default, Copy, Clone)]
pub struct Seg<T> {
    pub offset: u32,
    pub len: u32,
    _ty: PhantomData<T>,
}

impl<T> Seg<T> {
    pub fn new(offset: u32, len: u32) -> Seg<T> {
        Seg {
            offset: offset,
            len: len,
            _ty: PhantomData,
        }
    }
}

#[repr(C, packed)]
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Default, Copy, Clone)]
pub struct FileRecord {
    pub filename: Seg<u8>,
    pub comp_dir: Seg<u8>,
}

#[repr(C, packed)]
#[derive(Default, Copy, Clone)]
pub struct FuncRecord {
    /// low bits of the address.
    pub addr_low: u32,
    /// high bits of the address
    pub addr_high: u16,
    /// the length of the function.
    pub len: u16,
    /// The ID of the symbol of this function or ~0 if no symbol.
    pub symbol_id: u32,
    /// The ID of the parent function.  If the function has no
    /// parent then it will be ~0
    pub parent_id: u32,
    /// The line record of this function.  If it fully overlaps
    /// with an inline the record could be ~0
    pub line_record_id: u32,
}

#[repr(C, packed)]
#[derive(Default, Copy, Clone)]
pub struct LineRecord {
    /// offset to function item or line record
    pub addr_off: u16,
    /// absolutely indexed file
    pub file_id: u16,
    /// the line of the line record
    pub line: u16,
}

#[repr(C, packed)]
#[derive(Default, Copy, Clone)]
pub struct CacheFileHeader {
    pub version: u32,
    pub uuid: Uuid,
    pub arch: [i8; 16],
    pub symbols: Seg<Seg<u8>>,
    pub files: Seg<FileRecord>,
    pub function_records: Seg<FuncRecord>,
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

    pub fn parent(&self) -> Option<usize> {
        if self.parent_id == !0 {
            None
        } else {
            Some(self.parent_id as usize)
        }
    }
}
