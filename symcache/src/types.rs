use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::mem;
use std::slice;
use std::marker::PhantomData;

use uuid::Uuid;

use symbolic_debuginfo::DebugId;

#[repr(C, packed)]
#[derive(Default)]
pub struct Seg<T, L = u32> {
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

impl<T, L: Copy> Copy for Seg<T, L> {}

impl<T, L: Copy> Clone for Seg<T, L> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T, L> PartialEq for Seg<T, L> {
    fn eq(&self, other: &Self) -> bool {
        self.offset == other.offset
    }
}

impl<T, L> Eq for Seg<T, L> {}

impl<T, L> PartialOrd for Seg<T, L> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        { self.offset }.partial_cmp(&{ other.offset })
    }
}

impl<T, L> Ord for Seg<T, L> {
    fn cmp(&self, other: &Self) -> Ordering {
        { self.offset }.cmp(&{ other.offset })
    }
}

impl<T, L> Hash for Seg<T, L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        { self.offset }.hash(state);
    }
}

impl<T, L: fmt::Debug + Copy> fmt::Debug for Seg<T, L> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Seg")
            .field("offset", &{ self.offset })
            .field("len", &{ self.len })
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

#[derive(Debug, Fail, Copy, Clone)]
#[fail(display = "unknown symcache data source")]
pub struct UnknownDataSourceError;

#[repr(u32)]
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
pub enum DataSource {
    Unknown,
    Dwarf,
    SymbolTable,
    BreakpadSym,
    #[doc(hidden)]
    __Max,
}

impl DataSource {
    /// Creates a data soure from the u32 it represents
    pub fn from_u32(val: u32) -> Result<DataSource, UnknownDataSourceError> {
        if val >= (DataSource::__Max as u32) {
            Err(UnknownDataSourceError)
        } else {
            Ok(unsafe { mem::transmute(val as u32) })
        }
    }
}

impl Default for DataSource {
    fn default() -> DataSource {
        DataSource::Unknown
    }
}

#[repr(C, packed)]
#[derive(Default, Copy, Clone, Debug)]
pub struct CacheFilePreamble {
    pub magic: [u8; 4],
    pub version: u32,
}

pub trait CacheFileHeader {
    fn id(&self) -> DebugId;
    fn arch(&self) -> u32;
    fn data_source(&self) -> u8;
    fn has_line_records(&self) -> u8;
    fn symbols(&self) -> &Seg<Seg<u8, u16>>;
    fn files(&self) -> &Seg<FileRecord, u16>;
    fn function_records(&self) -> &Seg<FuncRecord>;
}

#[repr(C, packed)]
#[derive(Default, Copy, Clone, Debug)]
pub struct CacheFileHeaderV1 {
    pub preamble: CacheFilePreamble,
    pub uuid: Uuid,
    pub arch: u32,
    pub data_source: u8,
    pub has_line_records: u8,
    pub symbols: Seg<Seg<u8, u16>>,
    pub files: Seg<FileRecord, u16>,
    pub function_records: Seg<FuncRecord>,
}

impl CacheFileHeader for CacheFileHeaderV1 {
    fn id(&self) -> DebugId {
        DebugId::from_uuid(self.uuid)
    }

    fn arch(&self) -> u32 {
        self.arch
    }

    fn data_source(&self) -> u8 {
        self.data_source
    }

    fn has_line_records(&self) -> u8 {
        self.has_line_records
    }

    fn symbols(&self) -> &Seg<Seg<u8, u16>> {
        &self.symbols
    }

    fn files(&self) -> &Seg<FileRecord, u16> {
        &self.files
    }

    fn function_records(&self) -> &Seg<FuncRecord> {
        &self.function_records
    }
}

#[repr(C, packed)]
#[derive(Default, Copy, Clone, Debug)]
pub struct CacheFileHeaderV2 {
    pub preamble: CacheFilePreamble,
    pub id: DebugId,
    pub arch: u32,
    pub data_source: u8,
    pub has_line_records: u8,
    pub symbols: Seg<Seg<u8, u16>>,
    pub files: Seg<FileRecord, u16>,
    pub function_records: Seg<FuncRecord>,
}

impl CacheFileHeaderV2 {
    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            let bytes: *const u8 = mem::transmute(self);
            slice::from_raw_parts(bytes, mem::size_of::<CacheFileHeaderV2>())
        }
    }
}

impl CacheFileHeader for CacheFileHeaderV2 {
    fn id(&self) -> DebugId {
        self.id
    }

    fn arch(&self) -> u32 {
        self.arch
    }

    fn data_source(&self) -> u8 {
        self.data_source
    }

    fn has_line_records(&self) -> u8 {
        self.has_line_records
    }

    fn symbols(&self) -> &Seg<Seg<u8, u16>> {
        &self.symbols
    }

    fn files(&self) -> &Seg<FileRecord, u16> {
        &self.files
    }

    fn function_records(&self) -> &Seg<FuncRecord> {
        &self.function_records
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
            Some(func_id - (self.parent_offset as usize))
        }
    }
}
