use std::fmt;
use std::mem;
use std::slice;
use std::marker::PhantomData;

use uuid::Uuid;


#[repr(C, packed)]
#[derive(Eq, PartialEq, Ord, PartialOrd, Hash, Default, Copy, Clone)]
pub struct Seg<T, L=u32> {
    pub offset: u32,
    pub len: L,
    _ty: PhantomData<T>,
}

impl<T, L> Seg<T, L> {
    pub fn new(offset: u32, len: L) -> Seg<T, L> {
        Seg {
            offset: offset,
            len: len,
            _ty: PhantomData,
        }
    }
}

impl<T, L: fmt::Debug> fmt::Debug for Seg<T, L> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Seg")
            .field("offset", &self.offset)
            .field("len", &self.len)
            .finish()
    }
}

#[repr(C, packed)]
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Default, Copy, Clone, Debug)]
pub struct FileRecord {
    pub filename: Seg<u8, u8>,
    pub base_dir: Seg<u8, u8>,
}

#[repr(C, packed)]
#[derive(Default, Copy, Clone, Debug)]
pub struct FuncRecord {
    /// low bits of the address.
    pub addr_low: u32,
    /// high bits of the address
    pub addr_high: u16,
    /// the length of the function.
    pub len: u16,
    /// The line record of this function.  If it fully overlaps
    /// with an inline the record could be ~0
    pub line_records: Seg<LineRecord, u16>,
    /// The comp dir of the file record
    pub comp_dir: Seg<u8, u8>,
    /// The ID offset of the parent funciton.  Will be ~0 if the function has
    /// no parent.
    pub parent_offset: u16,
    /// The low bits of the ID of the symbol of this function or ~0 if no symbol.
    pub symbol_id_low: u16,
    /// The high bits of the ID of the symbol of this function or ~0 if no symbol.
    pub symbol_id_high: u8,
    /// The language of the func record.
    pub lang: u8,
}

#[repr(C, packed)]
#[derive(Default, Copy, Clone, Debug)]
pub struct LineRecord {
    /// offset to function item or line record
    pub addr_off: u8,
    /// absolutely indexed file
    pub file_id: u16,
    /// the line of the line record
    pub line: u16,
}

#[repr(C, packed)]
#[derive(Default, Copy, Clone, Debug)]
pub struct CacheFileHeader {
    pub magic: [u8; 4],
    pub version: u32,
    pub uuid: Uuid,
    pub arch: u32,
    pub symbols: Seg<Seg<u8, u16>>,
    pub files: Seg<FileRecord, u16>,
    pub function_records: Seg<FuncRecord>,
}

impl CacheFileHeader {
    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            let bytes: *const u8 = mem::transmute(self);
            slice::from_raw_parts(
                bytes,
                mem::size_of::<CacheFileHeader>()
            )
        }
    }
}

impl FuncRecord {
    pub fn symbol_id(&self) -> u32 {
        ((self.symbol_id_high as u32) << 16) | self.symbol_id_low as u32
    }

    pub fn addr_start(&self) -> u64 {
        ((self.addr_high as u64) << 32) | self.addr_low as u64
    }

    pub fn addr_end(&self) -> u64 {
        self.addr_start() + self.len as u64
    }

    pub fn addr_in_range(&self, addr: u64) -> bool {
        addr >= self.addr_start() && addr <= self.addr_end()
    }

    pub fn parent(&self, func_id: usize) -> Option<usize> {
        if self.parent_offset == !0 {
            None
        } else {
            Some(func_id + (self.parent_offset as usize))
        }
    }
}
