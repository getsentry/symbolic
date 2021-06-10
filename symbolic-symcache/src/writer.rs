use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::io::{self, Seek, Write};
use std::num::NonZeroU16;

use fnv::{FnvHashMap, FnvHashSet};

use symbolic_common::{Arch, DebugId, Language};
use symbolic_debuginfo::{DebugSession, FileInfo, Function, LineInfo, ObjectLike, Symbol};

use crate::error::{SymCacheError, SymCacheErrorKind, ValueKind};
use crate::format;

// Performs a shallow check whether this function might contain any lines.
fn is_empty_function(function: &Function<'_>) -> bool {
    function.size == 0
}

/// Recursively cleans a tree of functions that does not cover any lines.
///
///  - Removes all redundant line records
///  - Removes all empty functions (see [`is_empty_function`])
fn clean_function(function: &mut Function<'_>, line_cache: &mut LineCache) {
    function.inlinees.retain(|f| !is_empty_function(f));
    let mut inlinee_lines = LineCache::default();

    for inlinee in &mut function.inlinees {
        clean_function(inlinee, &mut inlinee_lines);
    }

    // Remove duplicate lines
    function
        .lines
        .retain(|l| inlinee_lines.insert((l.address, l.line)));

    line_cache.extend(inlinee_lines);
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
            .map_err(|e| SymCacheError::new(SymCacheErrorKind::WriteFailed, e))?;

        Ok(())
    }

    /// Writes the given bytes to the writer.
    #[inline]
    fn write_bytes(&mut self, data: &[u8]) -> Result<(), SymCacheError> {
        self.writer
            .write_all(data)
            .map_err(|e| SymCacheError::new(SymCacheErrorKind::WriteFailed, e))?;

        self.position += data.len() as u64;
        Ok(())
    }

    /// Writes a slice as binary data and returns a [`Seg`](format::Seg) pointing to the written data.
    ///
    /// This operation may fail if the length of the slice does not fit in the segment's index type.
    ///
    /// The data items are directly transmuted to their binary representation. Thus, they should not
    /// contain any references and have a stable memory layout (`#[repr(C, packed)]`).
    #[inline]
    fn write_segment<T, L>(
        &mut self,
        data: &[T],
        kind: ValueKind,
    ) -> Result<format::Seg<T, L>, SymCacheError>
    where
        L: Default + Copy + std::convert::TryFrom<usize>,
    {
        if data.is_empty() {
            return Ok(format::Seg::default());
        }

        let byte_size = std::mem::size_of_val(data);
        let bytes = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, byte_size) };

        let segment_pos = self.position as u32;
        let segment_len =
            L::try_from(data.len()).map_err(|_| SymCacheErrorKind::TooManyValues(kind))?;

        self.write_bytes(bytes)?;
        Ok(format::Seg::new(segment_pos, segment_len))
    }
}

/// Reference to a function within the `SymCacheWriter`.
#[derive(Copy, Clone, Debug, Eq, Ord, Hash, PartialEq, PartialOrd)]
struct FuncRef {
    /// The start address of the function.
    pub addr: u64,
    /// The original index in the functions array.
    pub index: u32,
}

impl FuncRef {
    /// Creates a new reference to a function.
    pub fn new(addr: u64, index: u32) -> Self {
        FuncRef { addr, index }
    }

    /// Creates an empty reference, equivalent to `None`.
    pub fn none() -> Self {
        FuncRef { addr: 0, index: !0 }
    }

    /// Returns the index as `usize`.
    pub fn as_usize(self) -> Option<usize> {
        if self.index == !0 {
            None
        } else {
            Some(self.index as usize)
        }
    }
}

impl Default for FuncRef {
    fn default() -> Self {
        Self::none()
    }
}

/// A function record along with its original position and reference to parent.
#[derive(Debug)]
struct FuncHandle {
    /// Original position in the writer.
    pub original: FuncRef,

    /// Reference to the original position of the parent function.
    pub parent: FuncRef,

    /// Data of this record.
    pub record: format::FuncRecord,
}

/// A cache for line record deduplication across inline functions.
type LineCache = FnvHashSet<(u64, u64)>;

/// A high level writer that can construct SymCaches.
///
/// When using this writer directly, make sure to call [`finish`](SymCacheWriter::finish)
/// at the end, so that all segments are
/// written to the underlying writer and the header is fixed up with the references. Since segments
/// are consecutive chunks of memory, this can only be done once at the end of the writing process.
pub struct SymCacheWriter<W> {
    writer: FormatWriter<W>,
    header: format::HeaderV2,
    files: Vec<format::FileRecord>,
    symbols: Vec<format::Seg<u8, u16>>,
    functions: Vec<FuncHandle>,
    path_cache: HashMap<Vec<u8>, format::Seg<u8, u8>>,
    file_cache: FnvHashMap<format::FileRecord, u16>,
    symbol_cache: HashMap<String, u32>,
    sorted: bool,
}

impl<W> SymCacheWriter<W>
where
    W: Write + Seek,
{
    /// Converts an entire object into a SymCache.
    ///
    /// Any object which implements [`ObjectLike`] can be written into a
    /// [`SymCache`](crate::SymCache) by this function.  This already implicictly
    /// calls [`SymCacheWriter::finish`], thus consuming the writer.
    pub fn write_object<'d, 'o, O>(object: &'o O, target: W) -> Result<W, SymCacheError>
    where
        O: ObjectLike<'d, 'o>,
        O::Error: std::error::Error + Send + Sync + 'static,
    {
        let mut writer = SymCacheWriter::new(target)?;

        writer.set_arch(object.arch());
        writer.set_debug_id(object.debug_id());

        let session = object
            .debug_session()
            .map_err(|e| SymCacheError::new(SymCacheErrorKind::BadDebugFile, e))?;

        for function in session.functions() {
            let function =
                function.map_err(|e| SymCacheError::new(SymCacheErrorKind::BadDebugFile, e))?;
            writer.add_function(function)?;
        }

        // Sort the files to efficiently add symbols from the symbol table in linear time
        // complexity. When the writer finishes, it will sort again with the added symbols.
        writer.ensure_sorted();

        let mut symbols = object.symbol_map().into_iter().peekable();

        // Add symbols from the symbol table. Since `add_symbol` mutates the internal `functions`
        // list, remember the current range to avoid handling a function twice.
        for index in 0..writer.functions.len() {
            if let Some(function) = writer.functions.get(index) {
                let address = function.original.addr;
                let end = address + function.record.len.get() as u64;

                // Consume all functions before and within this function. Only write the symbols
                // before the function and drop the rest.
                while symbols.peek().map_or(false, |s| s.address < end) {
                    let symbol = symbols.next().unwrap();
                    if symbol.address < address {
                        writer.add_symbol(symbol)?;
                    }
                }
            }
        }

        for symbol in symbols {
            writer.add_symbol(symbol)?;
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
            sorted: true,
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
        let len = match symbol.size {
            0 => u16::MAX,
            s => std::cmp::min(s, 0xffff) as u16,
        };

        // This unwrap cannot fail; size is nonzero by definition.
        let len = NonZeroU16::new(len).unwrap();

        let record = format::FuncRecord {
            addr_low: (symbol.address & 0xffff_ffff) as u32,
            addr_high: ((symbol.address >> 32) & 0xffff) as u16,
            len,
            symbol_id_low: (symbol_id & 0xffff) as u16,
            symbol_id_high: ((symbol_id >> 16) & 0xff) as u8,
            line_records: format::Seg::default(),
            parent_offset: !0, // amended during write_functions
            comp_dir: format::Seg::default(),
            lang: Language::Unknown as u8,
        };

        self.push_function(record, FuncRef::none())?;
        Ok(())
    }

    /// Cleans up a function by recursively removing all empty inlinees, then inserts it into
    /// the writer.
    ///
    /// Does nothing if the function is empty itself.
    /// Functions **must** be added in ascending order using this method. This emits a function
    /// record for this function and for each inlinee recursively.
    pub fn add_function(&mut self, mut function: Function<'_>) -> Result<(), SymCacheError> {
        // If we encounter a function without any instructions we just skip it.  This saves memory
        // and since we only care about instructions where we can actually crash this is a
        // reasonable optimization.
        if is_empty_function(&function) {
            return Ok(());
        }
        if self.has_function(&function) {
            return Ok(());
        }
        clean_function(&mut function, &mut LineCache::default());
        self.insert_function(&function, FuncRef::none())
    }

    /// Checks if the given function range was already registered
    fn has_function(&self, function: &Function<'_>) -> bool {
        if !self.sorted {
            return self.functions.iter().any(|f| {
                f.record.addr_start() == function.address
                    && f.record.addr_end() == function.end_address()
            });
        }
        match self
            .functions
            .binary_search_by_key(&function.address, |f| f.record.addr_start())
        {
            Ok(idx) => self.functions[idx].record.addr_end() == function.end_address(),
            _ => false,
        }
    }

    /// Persists all open segments to the writer and fixes up the header.
    pub fn finish(mut self) -> Result<W, SymCacheError> {
        self.header.functions = self.write_functions()?;

        let mut writer = self.writer;
        let mut header = self.header;

        header.symbols = writer.write_segment(&self.symbols, ValueKind::Symbol)?;
        header.files = writer.write_segment(&self.files, ValueKind::File)?;

        writer.seek(0)?;
        writer.write_bytes(format::as_slice(&header))?;

        Ok(writer.into_inner())
    }

    /// Writes a segment for a path and adds it to the [`path_cache`](Self::path_cache).
    ///
    /// Paths longer than
    /// 2^8 bytes will be shortened using [`shorten_path`](symbolic_common::shorten_path).
    fn write_path(&mut self, path: &[u8]) -> Result<format::Seg<u8, u8>, SymCacheError> {
        if let Some(segment) = self.path_cache.get(path) {
            return Ok(*segment);
        }

        // Path segments use u8 length indicators
        let unicode = String::from_utf8_lossy(path);
        let shortened = symbolic_common::shorten_path(&unicode, std::u8::MAX.into());
        let segment = self
            .writer
            .write_segment(shortened.as_bytes(), ValueKind::File)?;
        self.path_cache.insert(path.into(), segment);
        Ok(segment)
    }

    /// Inserts a file into the writer.
    ///
    /// This writes segments containing the file's name and base directory and combines them
    /// into a [`FileRecord`](format::FileRecord). The returned `index`
    /// is that `FileRecord`'s index in the [`files`](Self::files) vector.
    fn insert_file(&mut self, file: &FileInfo<'_>) -> Result<u16, SymCacheError> {
        let record = format::FileRecord {
            filename: self.write_path(file.name)?,
            base_dir: self.write_path(file.dir)?,
        };

        if let Some(index) = self.file_cache.get(&record) {
            return Ok(*index);
        }

        // TODO: Instead of failing hard when exceeding the maximum allowed number of files, we rather
        // emit `u16::MAX` which is already treated as a sentinel value for unknown file entries.
        if self.files.len() >= u16::MAX as usize {
            return Ok(u16::MAX);
        }

        let index = self.files.len() as u16;
        self.file_cache.insert(record, index);
        self.files.push(record);
        Ok(index)
    }

    /// Inserts a symbol into the writer.
    ///
    /// This writes a segment containing the symbol's name. The returned `index`
    /// is that segment's index in the [`symbols`](Self::symbols) vector. Names longer than 2^16
    /// bytes will be truncated.
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

        // NB: We only use 24 bits to encode symbol offsets in function records.
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

    /// Takes an iterator of [`LineInfo`]s and returns a vector containing [`LineRecord`](format::LineRecord)s
    /// for those lines whose address is between `start_address` and `end_address`.
    ///
    /// - If the difference between the addresses of two consecutive
    /// lines `L1` and `L2` is greater than 255, dummy line records with the same file and line
    /// information as L1 will be inserted between the two.
    ///
    /// - One call of this function will
    /// produce a maximum of 2^16 line records and will not produce line records with an address more than
    /// 2^16 bytes after the start address. If either of these limits is exceeded, the function will return
    /// early with the address of the first line that could not be processed; it is then up to
    /// the caller to call it again with that address as the new start address.
    fn take_lines(
        &mut self,
        lines: &mut std::iter::Peekable<std::slice::Iter<'_, LineInfo<'_>>>,
        start_address: u64,
        end_address: u64,
    ) -> Result<(Vec<format::LineRecord>, u64), SymCacheError> {
        let mut line_records = vec![];
        let mut last_address = start_address;
        let mut last_file = 0;
        let mut last_line = 0;

        while let Some(line) = lines.peek() {
            let file_id = self.insert_file(&line.file)?;

            // We have seen that swift can generate line records that lie outside of the function
            // start.  Why this happens is unclear but it happens with highly inlined function
            // calls.  Instead of panicking we want to just assume there is a single record at the
            // address of the function and in case there are more the offsets are just slightly off.
            let mut remaining_offset = Some(line.address.saturating_sub(last_address));

            // Line records store offsets relative to the previous line's address. If that offset
            // exceeds 255 (max u8 value), we write multiple line records to fill the gap.
            while let Some(offset) = remaining_offset {
                let (current_offset, rest) = if offset > 0xff {
                    (0xff, Some(offset - 0xff))
                } else {
                    (offset, None)
                };

                remaining_offset = rest;
                last_address += current_offset;

                // If there is a rest offset, then the current line record is just a filler. This
                // record still falls into the previous record's range, so we need to use the
                // previous record's information. Only if there is no rest, use the new information.
                if rest.is_none() {
                    last_file = file_id;
                    last_line = line.line.min(std::u16::MAX.into()) as u16;
                }

                // Check if we can still add a line record to this function without exceeding limits
                // of the physical format. Otherwise, do an early exit and let the caller iterate.
                let should_split_function = last_address - start_address > std::u16::MAX.into()
                    || line_records.len() >= std::u16::MAX.into();

                if should_split_function {
                    return Ok((line_records, last_address));
                }

                line_records.push(format::LineRecord {
                    addr_off: current_offset as u8,
                    file_id: last_file,
                    line: last_line,
                });
            }

            lines.next();
        }

        Ok((line_records, end_address))
    }

    /// Inserts a function into the writer and writes its line records.
    ///
    /// This function may produce multiple [`FuncRecord`](format::FuncRecord)s for one [`Function`] under two conditions:
    ///
    ///  1. Its address range exceeds 2^16 bytes. This makes it too large for the `len` field in
    ///     the function record.
    ///  2. There are more than 2^16 line records. This is larger than the index used for the line
    ///     segment.
    fn insert_function(
        &mut self,
        function: &Function<'_>,
        parent_ref: FuncRef,
    ) -> Result<(), SymCacheError> {
        let language = function.name.language();
        let symbol_id = self.insert_symbol(function.name.as_str().into())?;
        let comp_dir = self.write_path(function.compilation_dir)?;
        let lang = u8::try_from(language as u32)
            .map_err(|_| SymCacheErrorKind::ValueTooLarge(ValueKind::Language))?;

        let mut current_start_address = function.address;
        let mut lines = function.lines.iter().peekable();

        while current_start_address < function.end_address() {
            // Create line records for a part of the function.
            // - The first return value is the vector of created line records.
            // - If all line records were created, the second return value is equal to `function.end_address()`
            //   and the loop terminates. Otherwise it is the address of the first line record
            // that couldn't be created, which is where we have to start the next iteration.
            let (line_records, next_start_address) =
                self.take_lines(&mut lines, current_start_address, function.end_address())?;

            let line_records = self.writer.write_segment(&line_records, ValueKind::Line)?;
            if line_records.len > 0 {
                self.header.has_line_records = 1;
            }

            let len = std::cmp::min(next_start_address - current_start_address, 0xffff) as u16;
            debug_assert_ne!(
                len, 0,
                "While adding function {}: length must be positive",
                function.name
            );

            let len = match NonZeroU16::new(len) {
                Some(len) => len,
                None => break,
            };

            let record = format::FuncRecord {
                addr_low: (current_start_address & 0xffff_ffff) as u32,
                addr_high: ((current_start_address >> 32) & 0xffff) as u16,
                len,
                symbol_id_low: (symbol_id & 0xffff) as u16,
                symbol_id_high: ((symbol_id >> 16) & 0xff) as u8,
                parent_offset: !0,
                line_records,
                comp_dir,
                lang,
            };

            let function_ref = self.push_function(record, parent_ref)?;
            for inlinee in &function.inlinees {
                if inlinee.address >= current_start_address
                    && inlinee.end_address() <= next_start_address
                {
                    self.insert_function(inlinee, function_ref)?;
                }
            }

            current_start_address = next_start_address;
        }

        Ok(())
    }

    /// Adds a [`FuncRecord`](format::FuncRecord) to the writer.
    fn push_function(
        &mut self,
        record: format::FuncRecord,
        parent: FuncRef,
    ) -> Result<FuncRef, SymCacheError> {
        let functions = &mut self.functions;
        let addr = record.addr_start();

        // Functions are not written through `writer.write_segment`, so a manual check for the
        // maximum number of functions is necessary. This can later be asserted when writing
        // functions to the file.
        let index = functions.len();
        if index >= std::u32::MAX as usize {
            return Err(SymCacheErrorKind::ValueTooLarge(ValueKind::Function).into());
        }

        // For optimization purposes, remember if all functions appear in order. If not, parent
        // offsets need to be fixed up when writing to the file.
        if self.sorted && functions.last().map_or(false, |f| addr < f.original.addr) {
            self.sorted = false;
        }

        // Set the original index of this function as the current insert index. If functions need to
        // be sorted later, this index can be used to resolve parent references via binary search.
        let original = FuncRef::new(addr, index as u32);

        functions.push(FuncHandle {
            original,
            parent,
            record,
        });

        Ok(original)
    }

    /// Checks whether the functions in the writer are sorted by their start address and sorts them
    /// otherwise.
    fn ensure_sorted(&mut self) {
        // Only sort if functions do not already appear in order. They are sorted primarily by their
        // start address, and secondarily by the index in which they appeared originally in the
        // file.
        if !self.sorted {
            dmsort::sort_by_key(&mut self.functions, |handle| handle.original);
        }
    }

    /// Writes the functions that have been added to this writer.
    fn write_functions(&mut self) -> Result<format::Seg<format::FuncRecord>, SymCacheError> {
        if self.functions.is_empty() {
            return Ok(format::Seg::default());
        }

        // To compute parent offsets after that, one can simply binary search by the parent_ref
        // handle, allowing for efficient unique lookups.
        self.ensure_sorted();

        let functions = &self.functions;
        let segment = format::Seg::new(self.writer.position as u32, functions.len() as u32);

        for (index, function) in functions.iter().enumerate() {
            let parent_ref = function.parent;

            let parent_index = if self.sorted {
                // Since the list of functions was given in order, the original parent ref can be
                // trusted and used to calculate the parent offset.
                parent_ref.as_usize()
            } else {
                // The list of functions had to be sorted, so the parent ref must be resolved to its
                // new index. This lookup should never fail, but to avoid unsafe code it is coerced
                // into an option here.
                functions
                    .binary_search_by_key(&parent_ref, |h| h.original)
                    .ok()
            };

            // Calculate the offset to the parent function and put it into the record. This assumes
            // that the parent is always sorted before its childen, which is enforced by the
            // debuginfo function iterators.
            let mut record = function.record;
            if let Some(parent_index) = parent_index {
                debug_assert!(parent_index < index);

                let parent_offset = index - parent_index;
                if parent_offset >= std::u16::MAX.into() {
                    return Err(SymCacheErrorKind::ValueTooLarge(ValueKind::ParentOffset).into());
                }

                record.parent_offset = parent_offset as u16;
            }

            // Convert to raw bytes and output directly to the writer.
            let record_size = std::mem::size_of::<format::FuncRecord>();
            let ptr = &record as *const _ as *const u8;
            let bytes = unsafe { std::slice::from_raw_parts(ptr, record_size) };
            self.writer.write_bytes(bytes)?;
        }

        Ok(segment)
    }
}
