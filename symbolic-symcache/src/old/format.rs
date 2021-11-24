//! Definition of the binary format for SymCaches.

use std::cmp::Ordering;
use std::fmt;
use std::io;
use std::marker::PhantomData;
use std::num::NonZeroU16;

use symbolic_common::{DebugId, Uuid};

use crate::error::{SymCacheError, SymCacheErrorKind};

/// The magic file preamble to identify symcache files.
pub const SYMCACHE_MAGIC: [u8; 4] = *b"SYMC";

/// The latest version of the file format.
pub const SYMCACHE_VERSION: u32 = 6;

// Version history:
//
// 1: Initial implementation
// 2: PR #58:  Migrate from UUID to Debug ID
// 3: PR #148: Consider all PT_LOAD segments in ELF
// 4: PR #155: Functions with more than 65k line records
// 5: PR #221: Invalid inlinee nesting leading to wrong stack traces
// 6: PR #319: Correct line offsets and spacer line records

/// Loads binary data from a segment.
pub(crate) fn get_slice(data: &[u8], offset: usize, len: usize) -> Result<&[u8], io::Error> {
    let to = offset.wrapping_add(len);
    if to < offset || to > data.len() {
        Err(io::Error::new(io::ErrorKind::UnexpectedEof, "out of range"))
    } else {
        Ok(&data[offset..to])
    }
}

/// Returns a breakpad record from the SymCache.
#[inline(always)]
pub(crate) fn get_record<T>(data: &[u8], offset: usize) -> Result<&T, io::Error> {
    let record = get_slice(data, offset, std::mem::size_of::<T>())?;
    Ok(unsafe { &*(record.as_ptr() as *const T) })
}

/// Loads a slice of typed objects from a binary slice.
#[inline(always)]
pub(crate) fn as_slice<T>(data: &T) -> &[u8] {
    unsafe {
        let pointer = data as *const T as *const u8;
        std::slice::from_raw_parts(pointer, std::mem::size_of::<T>())
    }
}

/// A reference to a segment in the SymCache.
///
/// This is essentially a fat pointer into the cache,
/// comprising a memory location and a length. The memory region it
/// points to starts at byte [`offset`](Self::offset) and spans
/// `[len](Self::len) * size_of::<T>()`
/// bytes. `Seg` is generic in the
/// type of `len` so that a smaller counter can be used if it is known
/// ahead of time that the segment will contain few items.
#[repr(C, packed)]
pub struct Seg<T, L = u32> {
    /// Absolute file offset of this segment.
    pub offset: u32,
    /// Number of items in this segment.
    pub len: L,
    _ty: PhantomData<T>,
}

impl<T, L> Seg<T, L> {
    /// Creates a segment with specified offset and length.
    #[inline]
    pub fn new(offset: u32, len: L) -> Seg<T, L> {
        Seg {
            offset,
            len,
            _ty: PhantomData,
        }
    }
}

impl<T, L> Seg<T, L>
where
    L: Copy + Into<u64>,
{
    /// Reads this segment's data from the SymCache buffer.
    pub fn read<'a>(&self, data: &'a [u8]) -> Result<&'a [T], SymCacheError> {
        let offset = self.offset as usize;
        let len = self.len.into() as usize;
        let size = std::mem::size_of::<T>() * len;
        let slice = get_slice(data, offset, size)
            .map_err(|e| SymCacheError::new(SymCacheErrorKind::BadSegment, e))?;
        Ok(unsafe { std::slice::from_raw_parts(slice.as_ptr() as *const T, len) })
    }

    /// Reads a single element within a segment from the SymCache buffer.
    pub fn get<'a, U>(&self, data: &'a [u8], index: U) -> Result<Option<&'a T>, SymCacheError>
    where
        U: Into<u64>,
    {
        Ok(self.read(data)?.get(index.into() as usize))
    }
}

impl<L> Seg<u8, L>
where
    L: Copy + Into<u64>,
{
    /// Reads this segment's data from the SymCache buffer as a string.
    pub fn read_str<'a>(&self, data: &'a [u8]) -> Result<&'a str, SymCacheError> {
        let slice = self.read(data)?;
        let string = std::str::from_utf8(slice)
            .map_err(|e| SymCacheError::new(SymCacheErrorKind::BadSegment, e))?;
        Ok(string)
    }
}

impl<T, L> Default for Seg<T, L>
where
    L: Default,
{
    fn default() -> Self {
        Seg::new(0, L::default())
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

impl<T, L> std::hash::Hash for Seg<T, L> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        { self.offset }.hash(state);
    }
}

impl<T, L: fmt::Debug + Copy> fmt::Debug for Seg<T, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Seg")
            .field("offset", &{ self.offset })
            .field("len", &{ self.len })
            .finish()
    }
}

/// The path and name of a file referenced by line records.
#[repr(C, packed)]
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Default, Copy, Clone, Debug)]
pub struct FileRecord {
    /// Segment offset of the file name.
    pub filename: Seg<u8, u8>,
    /// Segment offset of the base directory.
    pub base_dir: Seg<u8, u8>,
}

/// A function or public symbol.
#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct FuncRecord {
    /// Low bits of the address.
    pub addr_low: u32,

    /// High bits of the address.
    pub addr_high: u16,

    /// The length of the function.
    ///
    /// A value of `0xffff` indicates that the size is unknown.
    /// We cannot cache any useful information for functions containing no instructions, so
    /// the length is always positive.
    pub len: NonZeroU16,

    /// The line record of this function.  If it fully overlaps with an inline the record could be
    /// `~0`.
    pub line_records: Seg<LineRecord, u16>,

    /// The comp dir of the file record.
    pub comp_dir: Seg<u8, u8>,

    /// The ID offset of the parent function.  Will be ~0 if the function has no parent.
    pub parent_offset: u16,

    /// The low bits of the ID of the symbol of this function or ~0 if no symbol.
    pub symbol_id_low: u16,

    /// The high bits of the ID of the symbol of this function or ~0 if no symbol.
    pub symbol_id_high: u8,

    /// The language of the func record.
    pub lang: u8,
}

impl FuncRecord {
    /// The index of the function or symbol name in the [`symbols`](Header::symbols) segment.
    pub fn symbol_id(&self) -> u32 {
        (u32::from(self.symbol_id_high) << 16) | u32::from(self.symbol_id_low)
    }

    /// The starting instruction address of the function.
    pub fn addr_start(&self) -> u64 {
        (u64::from(self.addr_high) << 32) | u64::from(self.addr_low)
    }

    /// The instruction address _after_ the end of the function.
    ///
    /// If the function's [`len`](FuncRecord::len) is [`u16::MAX`], we assume it extends all the way
    /// to the end of the file.
    pub fn addr_end(&self) -> u64 {
        match self.len.get() {
            0xffff => u64::MAX,
            len => self.addr_start() + u64::from(len),
        }
    }

    /// Checks whether the given address is covered by the function.
    ///
    /// If the function's [`len`](FuncRecord::len) is [`u16::MAX`], we assume it extends all the way
    /// to the end of the file.
    pub fn addr_in_range(&self, addr: u64) -> bool {
        addr >= self.addr_start() && addr < self.addr_end()
    }

    /// Resolves the index of the parent function in the [`functions`](Header::functions)
    /// segment, if this is an inlined function.
    pub fn parent(&self, func_id: usize) -> Option<usize> {
        if self.parent_offset == !0 {
            None
        } else {
            Some(func_id - (self.parent_offset as usize))
        }
    }
}

/// A mapping between an instruction address and file / line information.
#[repr(C, packed)]
#[derive(Default, Copy, Clone, Debug)]
pub struct LineRecord {
    /// Offset to the previous line record in the same function, or to the [function address](FuncRecord::addr_start)
    /// if this is the first line.
    pub addr_off: u8,

    /// Index of the file record in the [`files`](Header::files) segment.
    pub file_id: u16,

    /// The line number of the line record.
    pub line: u16,
}

/// The start of a SymCache file.
#[repr(C, packed)]
#[derive(Default, Copy, Clone, Debug)]
pub struct Preamble {
    /// Magic bytes, see `SYMCACHE_MAGIC`.
    pub magic: [u8; 4],
    /// Version of the SymCache file format.
    pub version: u32,
}

/// DEPRECATED. Header used by V1 SymCaches.
#[repr(C, packed)]
#[derive(Default, Copy, Clone, Debug)]
pub struct HeaderV1 {
    /// Version-independent preamble.
    pub preamble: Preamble,

    /// Unique identifier of the object file. Does not support PDBs.
    pub uuid: Uuid,

    /// CPU architecture of the object file.
    pub arch: u32,

    /// Type of debug information that was used to create this SymCache.
    pub data_source: u8,

    /// Flag, whether this cache has line records.
    pub has_line_records: u8,

    /// Segment containing symbol names.
    pub symbols: Seg<Seg<u8, u16>>,

    /// Segment containing [file records](FileRecord).
    pub files: Seg<FileRecord, u16>,

    /// Segment containing [function records](FuncRecord).
    pub functions: Seg<FuncRecord>,
}

/// Header used by V2 SymCaches.
#[repr(C, packed)]
#[derive(Default, Copy, Clone, Debug)]
pub struct HeaderV2 {
    /// Version-independent preamble.
    pub preamble: Preamble,

    /// Debug identifier of the object file.
    pub debug_id: DebugId,

    /// CPU architecture of the object file.
    pub arch: u32,

    /// DEPRECATED. Type of debug information that was used to create this SymCache.
    pub data_source: u8,

    /// Flag, whether this cache has line records.
    pub has_line_records: u8,

    /// Segment containing symbol names.
    pub symbols: Seg<Seg<u8, u16>>,

    /// Segment containing [file records](FileRecord).
    pub files: Seg<FileRecord, u16>,

    /// Segment containing [function records](FuncRecord).
    pub functions: Seg<FuncRecord>,
}

/// Version independent representation of the header.
#[derive(Clone, Debug)]
pub struct Header {
    /// Version-independent preamble.
    pub preamble: Preamble,

    /// Debug identifier of the object file.
    pub debug_id: DebugId,

    /// CPU architecture of the object file.
    pub arch: u32,

    /// DEPRECATED. Type of debug information that was used to create this SymCache.
    pub data_source: u8,

    /// Flag, whether this cache has line records.
    pub has_line_records: u8,

    /// Segment containing symbol names.
    pub symbols: Seg<Seg<u8, u16>>,

    /// Segment containing [file records](FileRecord).
    pub files: Seg<FileRecord, u16>,

    /// Segment containing [function records](FuncRecord).
    pub functions: Seg<FuncRecord>,
}

impl Header {
    /// Parses the correct version of the SymCache header.
    pub fn parse(data: &[u8]) -> Result<Self, SymCacheError> {
        let preamble = get_record::<Preamble>(data, 0)
            .map_err(|e| SymCacheError::new(SymCacheErrorKind::BadFileHeader, e))?;

        if preamble.magic != SYMCACHE_MAGIC {
            return Err(SymCacheErrorKind::BadFileMagic.into());
        }

        Ok(match preamble.version {
            1 => get_record::<HeaderV1>(data, 0)
                .map_err(|e| SymCacheError::new(SymCacheErrorKind::BadFileHeader, e))?
                .into(),
            2..=SYMCACHE_VERSION => get_record::<HeaderV2>(data, 0)
                .map_err(|e| SymCacheError::new(SymCacheErrorKind::BadFileHeader, e))?
                .into(),
            _ => return Err(SymCacheErrorKind::UnsupportedVersion.into()),
        })
    }
}

impl From<&'_ HeaderV1> for Header {
    fn from(header: &HeaderV1) -> Self {
        Header {
            preamble: header.preamble,
            debug_id: header.uuid.into(),
            arch: header.arch,
            data_source: header.data_source,
            has_line_records: header.has_line_records,
            symbols: header.symbols,
            files: header.files,
            functions: header.functions,
        }
    }
}

impl From<&'_ HeaderV2> for Header {
    fn from(header: &HeaderV2) -> Self {
        Header {
            preamble: header.preamble,
            debug_id: header.debug_id,
            arch: header.arch,
            data_source: header.data_source,
            has_line_records: header.has_line_records,
            symbols: header.symbols,
            files: header.files,
            functions: header.functions,
        }
    }
}
