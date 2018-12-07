use std::cell::RefCell;
use std::fmt;
use std::io;
use std::io::Write;
use std::mem;
use std::slice;
use std::str;

use failure::ResultExt;

use symbolic_common::byteview::ByteView;
use symbolic_common::types::{Arch, DebugId, Language, Name};
use symbolic_debuginfo::Object;
use symbolic_demangle::Demangle;

use crate::error::{SymCacheError, SymCacheErrorKind};
use crate::types::{
    CacheFileHeader, CacheFileHeaderV1, CacheFileHeaderV2, CacheFilePreamble, DataSource,
    FileRecord, FuncRecord, Seg,
};
use crate::utils::common_join_path;
use crate::writer;

/// The magic file preamble to identify symcache files.
pub const SYMCACHE_MAGIC: [u8; 4] = *b"SYMC";

/// The latest version of the file format.
pub const SYMCACHE_LATEST_VERSION: u32 = 2;

/// Information on a matched source line.
pub struct LineInfo<'a> {
    cache: &'a SymCache<'a>,
    sym_addr: u64,
    line_addr: u64,
    instr_addr: u64,
    line: u32,
    lang: Language,
    symbol: Option<&'a str>,
    filename: &'a str,
    base_dir: &'a str,
    comp_dir: &'a str,
}

/// An abstraction around a symbolication cache file.
pub struct SymCache<'a> {
    byteview: ByteView<'a>,
}

impl<'a> LineInfo<'a> {
    /// The architecture of the matched line.
    pub fn arch(&self) -> Arch {
        self.cache.arch().unwrap_or(Arch::Unknown)
    }

    /// The id of the matched line.
    pub fn id(&self) -> DebugId {
        self.cache.id().unwrap_or_default()
    }

    /// The instruction address where the function starts.
    pub fn sym_addr(&self) -> u64 {
        self.sym_addr
    }

    /// The instruction address where the line starts.
    pub fn line_addr(&self) -> u64 {
        self.line_addr
    }

    /// The actual instruction address.
    pub fn instr_addr(&self) -> u64 {
        self.instr_addr
    }

    /// The current line.
    pub fn line(&self) -> u32 {
        self.line
    }

    /// The current source code language.
    pub fn lang(&self) -> Language {
        self.lang
    }

    /// The string value of the symbol (mangled).
    pub fn symbol(&self) -> &'a str {
        self.symbol.unwrap_or("?")
    }

    /// The demangled function name.
    ///
    /// This demangles with default settings.  For further control the symbolic
    /// demangle crate can be manually used on the symbol.
    pub fn function_name(&self) -> String {
        Name::with_language(self.symbol(), self.lang()).try_demangle(Default::default())
    }

    /// The filename of the current line.
    pub fn filename(&self) -> &'a str {
        self.filename
    }

    /// The base dir of the current line.
    pub fn base_dir(&self) -> &str {
        self.base_dir
    }

    /// The fully joined file name.
    pub fn full_filename(&self) -> String {
        common_join_path(self.base_dir, self.filename)
    }

    /// The compilation dir of the function.
    pub fn comp_dir(&self) -> &'a str {
        self.comp_dir
    }
}

impl<'a> fmt::Display for LineInfo<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.function_name())?;
        if f.alternate() {
            let full_filename = self.full_filename();
            let line = self.line();
            let lang = self.lang();
            if full_filename != "" || line != 0 || lang != Language::Unknown {
                write!(f, "\n ")?;
                if full_filename != "" {
                    write!(f, " at {}", full_filename)?;
                }
                if line != 0 {
                    write!(f, " line {}", line)?;
                }
                if lang != Language::Unknown {
                    write!(f, " lang {}", lang)?;
                }
            }
        }
        Ok(())
    }
}

impl<'a> fmt::Debug for LineInfo<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LineInfo")
            .field("arch", &self.arch())
            .field("sym_addr", &self.sym_addr())
            .field("line_addr", &self.line_addr())
            .field("instr_addr", &self.instr_addr())
            .field("line", &self.line())
            .field("symbol", &self.symbol())
            .field("filename", &self.filename())
            .field("base_dir", &self.base_dir())
            .field("comp_dir", &self.comp_dir())
            .finish()
    }
}

/// A view of a single function in a `SymCache`.
pub struct Function<'a> {
    cache: &'a SymCache<'a>,
    id: u32,
    fun: &'a FuncRecord,
}

impl<'a> Function<'a> {
    /// The ID of the function.
    pub fn id(&self) -> usize {
        self.id as usize
    }

    /// The parent ID of the function.
    pub fn parent_id(&self) -> Option<usize> {
        self.fun.parent(self.id())
    }

    /// The address where the function starts.
    pub fn addr(&self) -> u64 {
        self.fun.addr_start()
    }

    /// The symbol of the function.
    pub fn symbol(&self) -> &str {
        self.cache
            .get_symbol(self.fun.symbol_id())
            .unwrap_or(None)
            .unwrap_or("?")
    }

    /// The demangled function name.
    ///
    /// This demangles with default settings.  For further control the symbolic
    /// demangle crate can be manually used on the symbol.
    pub fn function_name(&self) -> String {
        Name::with_language(self.symbol(), self.lang()).try_demangle(Default::default())
    }

    /// The language of the function.
    pub fn lang(&self) -> Language {
        Language::from_u32(self.fun.lang.into()).unwrap_or(Language::Unknown)
    }

    /// The compilation dir of the function.
    pub fn comp_dir(&self) -> &str {
        self.cache
            .get_segment_as_string(&self.fun.comp_dir)
            .unwrap_or("")
    }

    /// An iterator over all lines in the function.
    pub fn lines(&'a self) -> Lines<'a> {
        Lines {
            cache: self.cache,
            fun: &self.fun,
            addr: self.fun.addr_start(),
            idx: 0,
        }
    }
}

/// An iterator over all lines.
pub struct Lines<'a> {
    cache: &'a SymCache<'a>,
    fun: &'a FuncRecord,
    addr: u64,
    idx: usize,
}

/// Represents a single line.
pub struct Line<'a> {
    cache: &'a SymCache<'a>,
    addr: u64,
    line: u16,
    file_id: u16,
}

/// An iterator over all functions in a `SymCache`.
pub struct Functions<'a> {
    cache: &'a SymCache<'a>,
    idx: usize,
}

impl<'a> Iterator for Functions<'a> {
    type Item = Result<Function<'a>, SymCacheError>;

    fn next(&mut self) -> Option<Self::Item> {
        let records = match self.cache.function_records() {
            Ok(records) => records,
            Err(e) => return Some(Err(e)),
        };

        let function = match records.get(self.idx) {
            Some(function) => function,
            None => return None,
        };

        self.idx += 1;
        Some(Ok(Function {
            cache: self.cache,
            id: (self.idx - 1) as u32,
            fun: function,
        }))
    }
}

impl<'a> Iterator for Lines<'a> {
    type Item = Result<Line<'a>, SymCacheError>;

    fn next(&mut self) -> Option<Self::Item> {
        let segment = match self.cache.get_segment(&self.fun.line_records) {
            Ok(segment) => segment,
            Err(e) => return Some(Err(e)),
        };

        let record = match segment.get(self.idx) {
            Some(record) => record,
            None => return None,
        };

        self.idx += 1;
        self.addr += u64::from(record.addr_off);
        Some(Ok(Line {
            cache: self.cache,
            addr: self.addr,
            line: record.line,
            file_id: record.file_id,
        }))
    }
}

struct LineDebug<'a>(RefCell<Option<Lines<'a>>>);

impl<'a> fmt::Debug for LineDebug<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.0.borrow_mut().take().unwrap().filter_map(|x| x.ok()))
            .finish()
    }
}

impl<'a> fmt::Debug for Function<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Function")
            .field("id", &self.id())
            .field("parent_id", &self.parent_id())
            .field("symbol", &self.symbol())
            .field("addr", &self.addr())
            .field("comp_dir", &self.comp_dir())
            .field("lang", &self.lang())
            .field("lines()", &LineDebug(RefCell::new(Some(self.lines()))))
            .finish()
    }
}

impl<'a> fmt::Display for Function<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.function_name())?;
        if f.alternate() && self.lang() != Language::Unknown {
            write!(f, " [{}]", self.lang())?;
        }
        Ok(())
    }
}

impl<'a> fmt::Debug for Line<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Line")
            .field("addr", &self.addr())
            .field("line", &self.line())
            .field("base_dir", &self.base_dir())
            .field("filename", &self.filename())
            .finish()
    }
}

impl<'a> Line<'a> {
    /// The filename of the line.
    pub fn filename(&self) -> &str {
        if let Some(rec) = self.cache.get_file_record(self.file_id).unwrap_or(None) {
            self.cache
                .get_segment_as_string(&rec.filename)
                .unwrap_or("")
        } else {
            ""
        }
    }

    /// The base_dir of the line.
    pub fn base_dir(&self) -> &str {
        if let Some(rec) = self.cache.get_file_record(self.file_id).unwrap_or(None) {
            self.cache
                .get_segment_as_string(&rec.base_dir)
                .unwrap_or("")
        } else {
            ""
        }
    }

    /// The address of the line.
    pub fn addr(&self) -> u64 {
        self.addr
    }

    /// The line number of the line.
    pub fn line(&self) -> u16 {
        self.line
    }
}

impl<'a> SymCache<'a> {
    /// Load a symcache from a byteview.
    pub fn parse(byteview: ByteView<'a>) -> Result<Self, SymCacheError> {
        let rv = SymCache { byteview };
        {
            let preamble = rv.preamble()?;
            if preamble.magic != SYMCACHE_MAGIC {
                return Err(SymCacheErrorKind::BadFileMagic.into());
            }
            if preamble.version < 1 || preamble.version > SYMCACHE_LATEST_VERSION {
                return Err(SymCacheErrorKind::UnsupportedVersion.into());
            }
        }
        Ok(rv)
    }

    /// Constructs a symcache from an object.
    pub fn from_object(obj: &Object<'_>) -> Result<Self, SymCacheError> {
        let vec = writer::to_vec(obj)?;
        SymCache::parse(ByteView::from_vec(vec))
    }

    /// The total size of the cache file
    pub fn size(&self) -> usize {
        self.byteview.len()
    }

    /// Returns a pointer to the internal bytes of the cache file
    pub fn as_bytes(&self) -> &[u8] {
        &self.byteview
    }

    /// Writes the symcache into a new writer.
    pub fn to_writer<W: Write>(&self, mut writer: W) -> Result<(), io::Error> {
        io::copy(&mut &self.byteview[..], &mut writer)?;
        Ok(())
    }

    /// Loads binary data from a segment.
    fn get_data(&self, from: usize, len: usize) -> Result<&[u8], io::Error> {
        let buffer = &self.byteview;
        let to = from.wrapping_add(len);
        if to < from || to > buffer.len() {
            Err(io::Error::new(io::ErrorKind::UnexpectedEof, "out of range"))
        } else {
            Ok(&buffer[from..to])
        }
    }

    /// Returns the base offset for all segments
    fn get_segment_offset(&self) -> Result<usize, SymCacheError> {
        Ok(match self.preamble()?.version {
            1 => mem::size_of::<CacheFileHeaderV1>(),
            _ => 0,
        })
    }

    /// Loads data from a segment.
    fn get_segment<T, L: Copy + Into<u64>>(&self, seg: &Seg<T, L>) -> Result<&[T], SymCacheError> {
        let offset = seg.offset as usize + self.get_segment_offset()?;
        let len: u64 = seg.len.into();
        let len = len as usize;
        let size = mem::size_of::<T>() * len;
        let bytes = self
            .get_data(offset, size)
            .context(SymCacheErrorKind::BadSegment)?;
        Ok(unsafe { slice::from_raw_parts(bytes.as_ptr() as *const T, len) })
    }

    /// Returns a reference to the UTF8 representation of a segment.
    ///
    /// This will error if the segment contains non-UTF8 data.
    fn get_segment_as_string<L: Copy + Into<u64>>(
        &self,
        seg: &Seg<u8, L>,
    ) -> Result<&str, SymCacheError> {
        let bytes = self.get_segment(seg)?;
        Ok(str::from_utf8(bytes).context(SymCacheErrorKind::BadSegment)?)
    }

    /// Returns the SymCache preamble record.
    #[inline(always)]
    fn preamble(&self) -> Result<&CacheFilePreamble, SymCacheError> {
        let data = self
            .get_data(0, mem::size_of::<CacheFilePreamble>())
            .context(SymCacheErrorKind::BadFileHeader)?;

        Ok(unsafe { &*(data.as_ptr() as *const CacheFilePreamble) })
    }

    /// Returns the SymCache preamble record.
    #[inline(always)]
    fn header(&self) -> Result<&dyn CacheFileHeader, SymCacheError> {
        let preamble = self.preamble()?;

        match preamble.version {
            1 => {
                let data = self
                    .get_data(0, mem::size_of::<CacheFileHeaderV1>())
                    .context(SymCacheErrorKind::BadFileHeader)?;
                Ok(unsafe { &*(data.as_ptr() as *const CacheFileHeaderV1) })
            }
            2 => {
                let data = self
                    .get_data(0, mem::size_of::<CacheFileHeaderV2>())
                    .context(SymCacheErrorKind::BadFileHeader)?;
                Ok(unsafe { &*(data.as_ptr() as *const CacheFileHeaderV2) })
            }
            _ => Err(SymCacheErrorKind::UnsupportedVersion.into()),
        }
    }

    /// The architecture of the symbol file.
    pub fn arch(&self) -> Result<Arch, SymCacheError> {
        Ok(Arch::from_u32(self.header()?.arch()).context(SymCacheErrorKind::BadFileHeader)?)
    }

    /// The id of the cache file.
    pub fn id(&self) -> Result<DebugId, SymCacheError> {
        Ok(self.header()?.id())
    }

    /// The source of the `SymCache`.
    pub fn data_source(&self) -> Result<DataSource, SymCacheError> {
        Ok(DataSource::from_u32(self.header()?.data_source().into())
            .context(SymCacheErrorKind::BadFileHeader)?)
    }

    /// Returns true if line information is included.
    pub fn has_line_info(&self) -> Result<bool, SymCacheError> {
        Ok(self.header()?.has_line_records() != 0)
    }

    /// Returns true if file information is included.
    pub fn has_file_info(&self) -> Result<bool, SymCacheError> {
        Ok(match self.data_source()? {
            DataSource::Dwarf => self.has_line_info()?,
            _ => false,
        })
    }

    /// The version of the cache file.
    pub fn file_format_version(&self) -> Result<u32, SymCacheError> {
        Ok(self.preamble()?.version)
    }

    /// Looks up a single symbol.
    fn get_symbol(&self, idx: u32) -> Result<Option<&str>, SymCacheError> {
        if idx == !0 {
            return Ok(None);
        }
        let header = self.header()?;
        let syms = self.get_segment(header.symbols())?;
        if let Some(ref seg) = syms.get(idx as usize) {
            Ok(Some(self.get_segment_as_string(seg)?))
        } else {
            Ok(None)
        }
    }

    /// Resolves a `FuncRecord` from the functions segment.
    fn function_records(&'a self) -> Result<&'a [FuncRecord], SymCacheError> {
        let header = self.header()?;
        self.get_segment(header.function_records())
    }

    /// Resolves a `FileRecord` from the files segment.
    fn get_file_record(&self, idx: u16) -> Result<Option<&FileRecord>, SymCacheError> {
        // no match
        if idx == !0 {
            return Ok(None);
        }
        let header = self.header()?;
        let files = self.get_segment(header.files())?;
        Ok(files.get(idx as usize))
    }

    /// Locates the source line for an instruction address within a function.
    ///
    /// This function runs through all line records of the given function and
    /// returns the line closest to the specified instruction. `addr` must be
    /// within the function range, otherwise the response is implementation
    /// defined. However, `addr` may point to any address within an instruction.
    ///
    /// Returns some tuple containing:
    ///  - `.0`: The file containing the source code
    ///  - `.1`: First instruction address of the source line
    ///  - `.2`: Line number in the file
    ///
    /// Returns `None` if the function does not have line records.
    fn run_to_line(
        &'a self,
        fun: &'a FuncRecord,
        addr: u64,
    ) -> Result<Option<(&FileRecord, u64, u32)>, SymCacheError> {
        let records = self.get_segment(&fun.line_records)?;
        if records.is_empty() {
            // A non-empty function without line records can happen in a couple
            // of cases:
            //  1. There was no line information present while generating the
            //     symcache. This could be due to unsupported debug symbols or
            //     because they were stripped during the build process.
            //  2. The symbol was not pulled from debug info but a symbol table.
            //     such function records will generally have an estimated "size"
            //     but never line records.
            //  3. The body of this function consists of only inlined function
            //     calls. The actual line records of the address range will be
            //     found in the inlined `FuncRecord`s. The `SymCacheWriter` will
            //     try to emit synthetic line records in this case, but they
            //     will be missing if there is not enough debug information.
            return Ok(None);
        }

        // Because of how we determine the outer address on expanding
        // inlines the first address might actually already be missing
        // the record.  Because of that we pick in any case the first
        // record as fallback.
        let mut file_id = records[0].file_id;
        let mut line = u32::from(records[0].line);
        let mut running_addr = fun.addr_start() as u64;
        let mut line_addr = running_addr;

        for rec in records {
            // Keep running until we exceed the search address
            running_addr += u64::from(rec.addr_off);
            if running_addr > addr {
                break;
            }

            // Remember the starting address of the current line. There might be
            // multiple line records for a single line if `addr_off` overflows.
            // So only update `line_addr` if we actually hit a new line.
            if u32::from(rec.line) != line {
                line_addr = running_addr;
            }

            line = u32::from(rec.line);
            file_id = rec.file_id;
        }

        if let Some(ref record) = self.get_file_record(file_id)? {
            Ok(Some((record, line_addr, line)))
        } else {
            // This should not happen and indicates an invalid symcache
            Err(SymCacheErrorKind::BadCacheFile.into())
        }
    }

    /// Extracts source line information for an instruction address within the
    /// given `FuncRecord`.
    ///
    /// For parents of inlined frames, pass `Some(inner)` to `inner_sym`;
    /// otherwise None.
    ///
    /// This function tries to resolve the source file and line in which the
    /// corresponding instruction was defined and resolves the full path and
    /// file name.
    ///
    /// The location is first searched within the line records of this function.
    /// If the function has no own instructions (e.g. due to complete inlining),
    /// this information is taken from `inner_sym`. If that fails, the file and
    /// line information will be empty (0 or "").
    fn build_line_info(
        &'a self,
        fun: &'a FuncRecord,
        addr: u64,
        inner_sym: Option<&LineInfo<'a>>,
    ) -> Result<LineInfo<'a>, SymCacheError> {
        let (line, line_addr, filename, base_dir) =
            if let Some((file_record, line_addr, line)) = self.run_to_line(fun, addr)? {
                // The address was found in the function's line records, so use
                // it directly. This should is the default case for all valid
                // debugging information and the majority of all frames.
                (
                    line,
                    line_addr,
                    self.get_segment_as_string(&file_record.filename)?,
                    self.get_segment_as_string(&file_record.base_dir)?,
                )
            } else if let Some(inner_sym) = inner_sym {
                // The source line was not declared in this function. This
                // happens, if the function body consists of a single inlined
                // function call. Usually, the `SymCacheWriter` should emit a
                // synthetic line record in this case; but if debug symbols did
                // not provide sufficient information, we will still hit this
                // case. Use the inlined frame's source location as a
                // replacement to point somewhere useful.
                (
                    inner_sym.line,
                    inner_sym.line_addr,
                    inner_sym.filename,
                    inner_sym.base_dir,
                )
            } else {
                // We were unable to find any source code. This can happen for
                // synthetic functions, such as Swift method thunks. In that
                // case, we can only return empty line information. Also top-
                // level functions without line records pulled from the symbol
                // table will hit this branch.
                (0, 0, "", "")
            };

        Ok(LineInfo {
            cache: self,
            sym_addr: fun.addr_start(),
            line_addr,
            instr_addr: addr,
            line,
            lang: Language::from_u32(fun.lang.into()).unwrap_or(Language::Unknown),
            symbol: self.get_symbol(fun.symbol_id())?,
            filename,
            base_dir,
            comp_dir: self.get_segment_as_string(&fun.comp_dir)?,
        })
    }

    /// Returns an iterator over all functions.
    pub fn functions(&'a self) -> Functions<'a> {
        Functions {
            cache: self,
            idx: 0,
        }
    }

    /// Given an address this looks up the symbol at that point.
    ///
    /// Because of inling information this returns a vector of zero or
    /// more symbols.  If nothing is found then the return value will be
    /// an empty vector.
    pub fn lookup(&'a self, addr: u64) -> Result<Vec<LineInfo<'a>>, SymCacheError> {
        let funcs = self.function_records()?;

        // Functions in the function segment are ordered by start address
        // primarily and by depth secondarily.  As a result we want to have
        // a secondary comparison by the item index.
        let mut func_id = match funcs.binary_search_by_key(&addr, |x| x.addr_start()) {
            Ok(idx) => idx,
            Err(0) => return Ok(vec![]),
            Err(next_idx) => next_idx - 1,
        };

        // Seek forward to the deepest inlined function at the same address.
        while let Some(fun) = funcs.get(func_id + 1) {
            if fun.addr_start() != funcs[func_id].addr_start() {
                break;
            }
            func_id += 1;
        }

        let mut fun = &funcs[func_id];

        // The binary search matches the closest function that starts before our
        // search address. However, that function might end before that already,
        // for two reasons:
        //  1. It is inlined and one of the ancestors will contain the code. Try
        //     to move up the inlining hierarchy until we contain the address.
        //  2. There is a gap between the functions and the instruction is not
        //     covered by any of our function records.
        while !fun.addr_in_range(addr) {
            if let Some(parent_id) = fun.parent(func_id) {
                // Parent might contain the instruction (case 1)
                fun = &funcs[parent_id];
                func_id = parent_id;
            } else {
                // We missed entirely (case 2)
                return Ok(vec![]);
            }
        }

        let mut rv = vec![];

        // Line info for the direct (innermost) match
        rv.push(self.build_line_info(&fun, addr, None)?);

        // Line infos for all inlining ancestors, if any
        while let Some(parent_id) = fun.parent(func_id) {
            let outer_addr = fun.addr_start();
            fun = &funcs[parent_id];
            func_id = parent_id;
            let symbol = { self.build_line_info(&fun, outer_addr, Some(&rv[rv.len() - 1]))? };
            rv.push(symbol);
        }

        Ok(rv)
    }
}

impl<'a> fmt::Debug for SymCache<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SymCache")
            .field("id", &self.id())
            .field("size", &self.size())
            .field("arch", &self.arch().unwrap_or(Arch::Unknown))
            .field(
                "data_source",
                &self.data_source().unwrap_or(DataSource::Unknown),
            )
            .field("has_line_info", &self.has_line_info().unwrap_or(false))
            .field("has_file_info", &self.has_file_info().unwrap_or(false))
            .field(
                "functions",
                &self.function_records().map(|x| x.len()).unwrap_or(0),
            )
            .finish()
    }
}
