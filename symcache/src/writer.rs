use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{self, Seek, Write};

use failure::ResultExt;
use fnv::{FnvHashMap, FnvHashSet};

use symbolic_common::{Arch, DebugId, Language};
use symbolic_debuginfo::{DebugSession, FileInfo, Function, ObjectLike, Symbol};

use crate::error::{SymCacheError, SymCacheErrorKind, ValueKind};
use crate::format;

// Performs a shallow check whether this function might contain any lines.
fn is_empty_function(function: &Function<'_>) -> bool {
    function.lines.is_empty() && function.inlinees.is_empty()
}

/// Recursively cleans a tree of functions that does not cover any lines.
///
/// This first recursively cleans all inlinees and then removes those that have become empty.
fn clean_function(function: &mut Function<'_>) {
    for inlinee in &mut function.inlinees {
        clean_function(inlinee);
    }

    function.inlinees.retain(|f| !is_empty_function(f));
}

/// Low-level helper that writes segments and keeps track of the current offset.
struct FormatWriter<W> {
    writer: W,
    position: u64,
}

impl<W> FormatWriter<W>
where
    W: Write + Seek,
{
    /// Creates a new `FormatWriter`.
    fn new(writer: W) -> Self {
        FormatWriter {
            writer,
            position: 0,
        }
    }

    /// Unwraps the inner writer.
    fn into_inner(self) -> W {
        self.writer
    }

    /// Moves to the specified position.
    fn seek(&mut self, position: u64) -> Result<(), SymCacheError> {
        self.position = position;
        self.writer
            .seek(io::SeekFrom::Start(position))
            .context(SymCacheErrorKind::WriteFailed)?;

        Ok(())
    }

    /// Writes the given bytes to the writer.
    #[inline]
    fn write_bytes(&mut self, data: &[u8]) -> Result<(), SymCacheError> {
        self.writer
            .write_all(data)
            .context(SymCacheErrorKind::WriteFailed)?;

        self.position += data.len() as u64;
        Ok(())
    }

    /// Writes a segment as binary data to the writer and returns the [`Seg`] reference.
    ///
    /// This operation may fail if the data slice is too large to fit the segment. Each segment
    /// defines a data type for defining its length, which might not fit as many elements.
    ///
    /// The data items are directly transmuted to their binary representation. Thus, they should not
    /// contain any references and have a stable memory layout (`#[repr(C, packed)]).
    ///
    /// [`Seg`]: format/struct.Seg.html
    #[inline]
    fn write_segment<T, L>(
        &mut self,
        data: &[T],
        kind: ValueKind,
    ) -> Result<format::Seg<T, L>, SymCacheError>
    where
        L: Default + Copy + num::FromPrimitive,
    {
        if data.is_empty() {
            return Ok(format::Seg::default());
        }

        let byte_size = std::mem::size_of_val(data);
        let bytes = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, byte_size) };

        let segment_pos = self.position as u32;
        let segment_len =
            L::from_usize(data.len()).ok_or_else(|| SymCacheErrorKind::TooManyValues(kind))?;

        self.write_bytes(bytes)?;
        Ok(format::Seg::new(segment_pos, segment_len))
    }
}

/// A high level writer that can construct SymCaches.
///
/// When using this writer directly, ensure to call [`finish`] at the end, so that all segments are
/// written to the underlying writer and the header is fixed up with the references. Since segments
/// are consecutive chunks of memory, this can only be done once at the end of the writing process.
///
/// [`finish`]: struct.SymCacheWriter.html#method.finish
pub struct SymCacheWriter<W> {
    writer: FormatWriter<W>,
    header: format::HeaderV2,
    files: Vec<format::FileRecord>,
    symbols: Vec<format::Seg<u8, u16>>,
    functions: Vec<format::FuncRecord>,
    path_cache: HashMap<String, format::Seg<u8, u8>>,
    file_cache: FnvHashMap<format::FileRecord, u16>,
    symbol_cache: HashMap<String, u32>,
}

impl<W> SymCacheWriter<W>
where
    W: Write + Seek,
{
    /// Converts an entire object into a SymCache.
    pub fn write_object<O>(object: &O, target: W) -> Result<W, SymCacheError>
    where
        O: ObjectLike,
    {
        let mut writer = SymCacheWriter::new(target).context(SymCacheErrorKind::WriteFailed)?;

        writer.set_arch(object.arch());
        writer.set_debug_id(object.debug_id());

        let mut last_address = 0;
        let mut symbols = object.symbol_map().into_iter().peekable();
        let mut session = object
            .debug_session()
            .context(SymCacheErrorKind::BadDebugFile)?;

        let functions = session
            .functions()
            .context(SymCacheErrorKind::BadDebugFile)?;

        for function in functions {
            while symbols
                .peek()
                .map_or(false, |s| s.address < function.address)
            {
                let symbol = symbols.next().unwrap();
                if symbol.address >= last_address {
                    writer.add_symbol(symbol)?;
                }
            }

            // Ensure that symbols at the function address are skipped even if the function size is
            // zero. We trust that the function range (address to address + size) spans all lines.
            last_address = function.address + std::cmp::max(1, function.size);
            writer.add_function(function)?;
        }

        for symbol in symbols {
            if symbol.address > last_address {
                writer.add_symbol(symbol)?;
            }
        }

        writer.finish()
    }

    /// Constructs a new `SymCacheWriter` and writes the preamble.
    pub fn new(writer: W) -> Result<Self, SymCacheError> {
        let mut header = format::HeaderV2::default();
        header.preamble.magic = format::SYMCACHE_MAGIC;
        header.preamble.version = format::SYMCACHE_VERSION;

        let mut writer = FormatWriter::new(writer);
        writer.seek(std::mem::size_of_val(&header) as u64)?;

        Ok(SymCacheWriter {
            writer,
            header,
            files: Vec::new(),
            symbols: Vec::new(),
            functions: Vec::new(),
            path_cache: HashMap::new(),
            file_cache: FnvHashMap::default(),
            symbol_cache: HashMap::new(),
        })
    }

    /// Sets the CPU architecture of this SymCache.
    pub fn set_arch(&mut self, arch: Arch) {
        self.header.arch = arch as u32;
    }

    /// Sets the debug identifier of this SymCache.
    pub fn set_debug_id(&mut self, debug_id: DebugId) {
        self.header.debug_id = debug_id;
    }

    /// Adds a new symbol to this SymCache.
    ///
    /// Symbols **must** be added in ascending order using this method. This will emit a function
    /// record internally.
    pub fn add_symbol(&mut self, symbol: Symbol<'_>) -> Result<(), SymCacheError> {
        let name = match symbol.name {
            Some(name) => name,
            None => return Ok(()),
        };

        let symbol_id = self.insert_symbol(name)?;

        // NB: SymbolMap usually fills in sizes of consecutive symbols already. This is not done if
        // there is only one symbol and for the last symbol. `FuncRecord::addr_in_range` always
        // requires some address range. Since we can't possibly know the actual size, just assume
        // that the symbol is VERY large.
        let size = match symbol.size {
            0 => !0,
            s => s,
        };

        self.functions.push(format::FuncRecord {
            addr_low: (symbol.address & 0xffff_ffff) as u32,
            addr_high: ((symbol.address >> 32) & 0xffff) as u16,
            // XXX: we have not seen this yet, but in theory this should be
            // stored as multiple function records.
            len: std::cmp::min(size, 0xffff) as u16,
            symbol_id_low: (symbol_id & 0xffff) as u16,
            symbol_id_high: ((symbol_id >> 16) & 0xff) as u8,
            line_records: format::Seg::default(),
            parent_offset: !0,
            comp_dir: format::Seg::default(),
            lang: Language::Unknown as u8,
        });

        Ok(())
    }

    /// Adds a function to this SymCache.
    ///
    /// Functions **must** be added in ascending order using this method. This emits a function
    /// record for this function and for each inlinee recursively.
    pub fn add_function(&mut self, mut function: Function<'_>) -> Result<(), SymCacheError> {
        // If we encounter a function without any instructions we just skip it.  This saves memory
        // and since we only care about instructions where we can actually crash this is a
        // reasonable optimization.
        clean_function(&mut function);
        if is_empty_function(&function) {
            return Ok(());
        }

        self.insert_function(function, !0, &mut FnvHashSet::default())
    }

    /// Persists all open segments to the writer and fixes up the header.
    pub fn finish(self) -> Result<W, SymCacheError> {
        let mut writer = self.writer;
        let mut header = self.header;

        header.symbols = writer.write_segment(&self.symbols, ValueKind::Symbol)?;
        header.files = writer.write_segment(&self.files, ValueKind::File)?;
        header.functions = writer.write_segment(&self.functions, ValueKind::Function)?;

        writer.seek(0)?;
        writer.write_bytes(format::as_slice(&header))?;

        Ok(writer.into_inner())
    }

    fn write_path(&mut self, path: Cow<'_, str>) -> Result<format::Seg<u8, u8>, SymCacheError> {
        if let Some(segment) = self.path_cache.get(path.as_ref()) {
            return Ok(*segment);
        }

        // Path segments use u8 length indicators
        let shortened = symbolic_common::shorten_path(&path, std::u8::MAX.into());
        let segment = self
            .writer
            .write_segment(shortened.as_bytes(), ValueKind::File)?;
        self.path_cache.insert(path.into_owned(), segment);
        Ok(segment)
    }

    fn insert_file(&mut self, file: FileInfo<'_>) -> Result<u16, SymCacheError> {
        let record = format::FileRecord {
            filename: self.write_path(file.name)?,
            base_dir: self.write_path(file.dir)?,
        };

        if let Some(index) = self.file_cache.get(&record) {
            return Ok(*index);
        }

        if self.files.len() >= std::u16::MAX as usize {
            return Err(SymCacheErrorKind::TooManyValues(ValueKind::File).into());
        }

        let index = self.files.len() as u16;
        self.file_cache.insert(record, index);
        self.files.push(record);
        Ok(index)
    }

    fn insert_symbol(&mut self, name: Cow<'_, str>) -> Result<u32, SymCacheError> {
        let mut len = std::cmp::min(name.len(), std::u16::MAX.into());
        if len < name.len() {
            len = match std::str::from_utf8(name[..len].as_bytes()) {
                Ok(_) => len,
                Err(error) => error.valid_up_to(),
            };
        }

        if let Some(index) = self.symbol_cache.get(&name[..len]) {
            return Ok(*index);
        }

        // NB: We only use 48 bits to encode symbol offsets in function records.
        if self.symbols.len() >= 0x00ff_ffff {
            return Err(SymCacheErrorKind::TooManyValues(ValueKind::Symbol).into());
        }

        // Avoid a potential reallocation by reusing name.
        let mut name = name.into_owned();
        name.truncate(len);

        let segment = self
            .writer
            .write_segment(name.as_bytes(), ValueKind::Symbol)?;
        let index = self.symbols.len() as u32;
        self.symbols.push(segment);
        self.symbol_cache.insert(name, index);
        Ok(index)
    }

    fn insert_function(
        &mut self,
        function: Function<'_>,
        parent_index: u32,
        line_cache: &mut FnvHashSet<(u64, u64)>,
    ) -> Result<(), SymCacheError> {
        let index = self.functions.len() as u32;
        let address = function.address;
        let language = function.name.language();
        let symbol_id = self.insert_symbol(function.name.into_cow())?;
        let comp_dir = self.write_path(function.compilation_dir)?;

        let lang = if language as u32 > 0xff {
            return Err(SymCacheErrorKind::ValueTooLarge(ValueKind::Language).into());
        } else {
            language as u8
        };

        let parent_offset = if parent_index == !0 {
            !0
        } else {
            let parent_offset = index.saturating_sub(parent_index);
            if parent_offset > std::u16::MAX.into() {
                return Err(SymCacheErrorKind::ValueTooLarge(ValueKind::ParentOffset).into());
            }
            parent_offset as u16
        };

        self.functions.push(format::FuncRecord {
            addr_low: (address & 0xffff_ffff) as u32,
            addr_high: ((address >> 32) & 0xffff) as u16,
            // XXX: we have not seen this yet, but in theory this should be
            // stored as multiple function records.
            len: std::cmp::min(function.size, 0xffff) as u16,
            symbol_id_low: (symbol_id & 0xffff) as u16,
            symbol_id_high: ((symbol_id >> 16) & 0xff) as u8,
            line_records: format::Seg::default(),
            parent_offset,
            comp_dir,
            lang,
        });

        // Recurse first including inner line records. Address rejection will prune duplicate line
        // records later on.
        for inlinee in function.inlinees {
            self.insert_function(inlinee, index, line_cache)?;
        }

        let mut last_address = address;
        let mut line_segment = vec![];
        for line in function.lines {
            // Reject this line if it has been covered by one of the inlinees.
            if !line_cache.insert((line.address, line.line)) {
                continue;
            }

            let file_id = self.insert_file(line.file)?;

            // We have seen that swift can generate line records that lie outside of the function
            // start.  Why this happens is unclear but it happens with highly inlined function
            // calls.  Instead of panicking we want to just assume there is a single record at the
            // address of the function and in case there are more the offsets are just slightly off.
            let mut diff = (line.address.saturating_sub(last_address)) as i64;

            // Line records store relative offsets to the previous line's address. If that offset
            // exceeds 255 (max u8 value), we write multiple line records to fill the gap.
            while diff >= 0 {
                let line_record = format::LineRecord {
                    addr_off: (diff & 0xff) as u8,
                    file_id,
                    line: std::cmp::min(line.line, std::u16::MAX.into()) as u16,
                };
                last_address += u64::from(line_record.addr_off);
                line_segment.push(line_record);
                diff -= 0xff;
            }
        }

        if !line_segment.is_empty() {
            self.functions[index as usize].line_records =
                self.writer.write_segment(&line_segment, ValueKind::Line)?;
            self.header.has_line_records = 1;
        }

        Ok(())
    }
}
