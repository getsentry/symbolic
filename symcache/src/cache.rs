use std::io;
use std::io::Write;
use std::mem;
use std::str;
use std::fmt;
use std::slice;
use std::cell::RefCell;

use uuid::Uuid;

use symbolic_common::{Arch, ByteView, ErrorKind, Language, Result};
use symbolic_debuginfo::Object;
use symbolic_demangle::demangle;

use types::{CacheFileHeader, DataSource, FileRecord, FuncRecord, Seg};
use utils::common_join_path;
use writer;

/// The magic file header to identify symcache files.
pub const SYMCACHE_MAGIC: [u8; 4] = [b'S', b'Y', b'M', b'C'];

/// The latest version of the file format.
pub const SYMCACHE_LATEST_VERSION: u32 = 1;

/// Information on a matched source line
pub struct LineInfo<'a> {
    cache: &'a SymCache<'a>,
    sym_addr: u64,
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

    /// The uuid of the matched line.
    pub fn uuid(&self) -> Uuid {
        self.cache.uuid().unwrap_or(Uuid::nil())
    }

    /// The instruction address where the line starts.
    pub fn sym_addr(&self) -> u64 {
        self.sym_addr
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
        demangle(self.symbol())
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("LineInfo")
            .field("arch", &self.arch())
            .field("sym_addr", &self.sym_addr())
            .field("instr_addr", &self.instr_addr())
            .field("line", &self.line())
            .field("symbol", &self.symbol())
            .field("filename", &self.filename())
            .field("base_dir", &self.base_dir())
            .field("comp_dir", &self.comp_dir())
            .finish()
    }
}

/// A view of a single function in a sym cache.
pub struct Function<'a> {
    cache: &'a SymCache<'a>,
    id: u32,
    fun: &'a FuncRecord,
}

impl<'a> Function<'a> {
    /// The ID of the function
    pub fn id(&self) -> usize {
        self.id as usize
    }

    /// The parent ID of the function
    pub fn parent_id(&self) -> Option<usize> {
        self.fun.parent(self.id())
    }

    /// The address where the function starts.
    pub fn addr(&self) -> u64 {
        self.fun.addr_start()
    }

    /// The symbol of the function.
    pub fn symbol(&self) -> &str {
        self.cache.get_symbol(self.fun.symbol_id()).unwrap_or(None).unwrap_or("?")
    }

    /// The demangled function name.
    ///
    /// This demangles with default settings.  For further control the symbolic
    /// demangle crate can be manually used on the symbol.
    pub fn function_name(&self) -> String {
        demangle(self.symbol())
    }

    /// The language of the function
    pub fn lang(&self) -> Language {
        Language::from_u32(self.fun.lang as u32).unwrap_or(Language::Unknown)
    }

    /// The compilation dir of the function
    pub fn comp_dir(&self) -> &str {
        self.cache.get_segment_as_string(&self.fun.comp_dir).unwrap_or("")
    }

    /// An iterator over all lines in the function
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

/// An iterator over all functions in a sym cache.
pub struct Functions<'a> {
    cache: &'a SymCache<'a>,
    idx: usize,
}

impl<'a> Iterator for Functions<'a> {
    type Item = Result<Function<'a>>;

    fn next(&mut self) -> Option<Result<Function<'a>>> {
        let records = itry!(self.cache.function_records());
        if let Some(fun) = records.get(self.idx) {
            self.idx += 1;
            Some(Ok(Function {
                cache: self.cache,
                id: (self.idx - 1) as u32,
                fun: fun,
            }))
        } else {
            None
        }
    }
}

impl<'a> Iterator for Lines<'a> {
    type Item = Result<Line<'a>>;

    fn next(&mut self) -> Option<Result<Line<'a>>> {
        let records = itry!(self.cache.get_segment(&self.fun.line_records));
        if let Some(rec) = records.get(self.idx) {
            self.idx += 1;
            self.addr += rec.addr_off as u64;
            Some(Ok(Line {
                cache: self.cache,
                addr: self.addr,
                line: rec.line,
                file_id: rec.file_id,
            }))
        } else {
            None
        }
    }
}

struct LineDebug<'a>(RefCell<Option<Lines<'a>>>);

impl<'a> fmt::Debug for LineDebug<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_list()
            .entries(self.0.borrow_mut().take().unwrap().filter_map(|x| x.ok()))
            .finish()
    }
}

impl<'a> fmt::Debug for Function<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.function_name())?;
        if f.alternate() && self.lang() != Language::Unknown {
            write!(f, " [{}]", self.lang())?;
        }
        Ok(())
    }
}

impl<'a> fmt::Debug for Line<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
            self.cache.get_segment_as_string(&rec.filename).unwrap_or("")
        } else {
            ""
        }
    }

    /// The base_dir of the line.
    pub fn base_dir(&self) -> &str {
        if let Some(rec) = self.cache.get_file_record(self.file_id).unwrap_or(None) {
            self.cache.get_segment_as_string(&rec.base_dir).unwrap_or("")
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
    pub fn new(byteview: ByteView<'a>) -> Result<SymCache<'a>> {
        let rv = SymCache {
            byteview: byteview,
        };
        {
            let header = rv.header()?;
            if header.magic != SYMCACHE_MAGIC {
                return Err(ErrorKind::BadCacheFile("Bad file magic").into());
            }
            if header.version != 1 {
                return Err(ErrorKind::BadCacheFile("Unsupported file version").into());
            }
        }
        Ok(rv)
    }

    /// Constructs a symcache from an object.
    pub fn from_object(obj: &Object) -> Result<SymCache<'a>> {
        let vec = writer::to_vec(obj)?;
        SymCache::new(ByteView::from_vec(vec))
    }

    /// The total size of the cache file
    pub fn size(&self) -> usize {
        self.byteview.len()
    }

    /// Returns a pointer to the internal bytes of the cache file
    pub fn as_bytes(&self) -> &[u8] {
        &self.byteview
    }

    /// Write the symcache into a new writer.
    pub fn to_writer<W: Write>(&self, mut writer: W) -> Result<()> {
        io::copy(&mut &self.byteview[..], &mut writer)?;
        Ok(())
    }

    fn get_data(&self, start: usize, len: usize) -> Result<&[u8]> {
        let buffer = &self.byteview;
        let end = start.wrapping_add(len);
        if end < start || end > buffer.len() {
            Err(
                io::Error::new(io::ErrorKind::UnexpectedEof, "out of range").into(),
            )
        } else {
            Ok(&buffer[start..end])
        }
    }

    fn get_segment<T, L: Copy + Into<u64>>(&self, seg: &Seg<T, L>) -> Result<&[T]> {
        let offset = seg.offset as usize +
            mem::size_of::<CacheFileHeader>() as usize;
        let len: u64 = seg.len.into();
        let len = len as usize;
        let size = mem::size_of::<T>() * len;
        unsafe {
            let bytes = self.get_data(offset, size)?;
            Ok(slice::from_raw_parts(
                mem::transmute(bytes.as_ptr()),
                len
            ))
        }
    }

    fn get_segment_as_string<L: Copy + Into<u64>>(&self, seg: &Seg<u8, L>) -> Result<&str> {
        let bytes = self.get_segment(seg)?;
        Ok(str::from_utf8(bytes)?)
    }

    #[inline(always)]
    fn header(&self) -> Result<&CacheFileHeader> {
        unsafe {
            Ok(mem::transmute(self.get_data(
                0, mem::size_of::<CacheFileHeader>())?.as_ptr()))
        }
    }

    /// The architecture of the cache file
    pub fn arch(&self) -> Result<Arch> {
        Arch::from_u32(self.header()?.arch)
    }

    /// The uuid of the cache file.
    pub fn uuid(&self) -> Result<Uuid> {
        Ok(self.header()?.uuid)
    }

    /// The source of the sym cache.
    pub fn data_source(&self) -> Result<DataSource> {
        DataSource::from_u32(self.header()?.data_source as u32)
    }

    /// Returns true if line information is included.
    pub fn has_line_info(&self) -> Result<bool> {
        Ok(self.header()?.has_line_records != 0)
    }

    /// Returns true if file information is included.
    pub fn has_file_info(&self) -> Result<bool> {
        Ok(match self.data_source()? {
            DataSource::Dwarf => self.has_line_info()?,
            _ => false,
        })
    }

    /// The version of the cache file.
    pub fn file_format_version(&self) -> Result<u32> {
        Ok(self.header()?.version)
    }

    /// Looks up a single symbol
    fn get_symbol(&self, idx: u32) -> Result<Option<&str>> {
        if idx == !0 {
            return Ok(None);
        }
        let header = self.header()?;
        let syms = self.get_segment(&header.symbols)?;
        if let Some(ref seg) = syms.get(idx as usize) {
            Ok(Some(self.get_segment_as_string(seg)?))
        } else {
            Ok(None)
        }
    }

    fn function_records(&'a self) -> Result<&'a [FuncRecord]> {
        let header = self.header()?;
        self.get_segment(&header.function_records)
    }

    fn get_file_record(&self, idx: u16) -> Result<Option<&FileRecord>> {
        // no match
        if idx == !0 {
            return Ok(None);
        }
        let header = self.header()?;
        let files = self.get_segment(&header.files)?;
        Ok(files.get(idx as usize))
    }

    fn run_to_line(&'a self, fun: &'a FuncRecord, addr: u64)
        -> Result<Option<(&FileRecord, u32)>>
    {
        let records = self.get_segment(&fun.line_records)?;

        if records.is_empty() {
            return Ok(None);
        }

        // because of how we determine the outer address on expanding
        // inlines the first address might actually already be missing
        // the record.  Because of that we pick in any case the first
        // record as fallback.
        let mut file_id = records[0].file_id;
        let mut line = records[0].line as u32;
        let mut running_addr = fun.addr_start() as u64;

        for rec in records {
            let new_instr = running_addr + rec.addr_off as u64;
            if new_instr > addr {
                break;
            }
            running_addr = new_instr;
            line = rec.line as u32;
            file_id = rec.file_id;
        }

        if let Some(ref record) = self.get_file_record(file_id)? {
            Ok(Some((record, line)))
        } else {
            Err(ErrorKind::Internal("unknown file id").into())
        }
    }

    fn build_symbol(&'a self, fun: &'a FuncRecord, addr: u64,
                    inner_sym: Option<&LineInfo<'a>>) -> Result<LineInfo<'a>> {
        let (line, filename, base_dir) = match self.run_to_line(fun, addr)? {
            Some((file_record, line)) => {
                (
                    line,
                    self.get_segment_as_string(&file_record.filename)?,
                    self.get_segment_as_string(&file_record.base_dir)?,
                )
            }
            None => {
                if let Some(inner_sym) = inner_sym {
                    (inner_sym.line, inner_sym.filename, inner_sym.base_dir)
                } else {
                    (0, "", "")
                }
            }
        };
        Ok(LineInfo {
            cache: self,
            sym_addr: fun.addr_start(),
            instr_addr: addr,
            line: line,
            lang: Language::from_u32(fun.lang as u32).unwrap_or(Language::Unknown),
            symbol: self.get_symbol(fun.symbol_id())?,
            filename: filename,
            base_dir: base_dir,
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
    pub fn lookup(&'a self, addr: u64) -> Result<Vec<LineInfo<'a>>> {
        let funcs = self.function_records()?;

        // functions in the function segment are ordered by start address
        // primarily and by depth secondarily.  As a result we want to have
        // a secondary comparison by the item index.
        let mut func_id = match funcs.binary_search_by_key(&addr, |x| x.addr_start()) {
            Ok(idx) => idx,
            Err(0) => return Ok(vec![]),
            Err(next_idx) => next_idx - 1,
        };

        // seek forward to the deepest inlined function at the same address.
        while let Some(fun) = funcs.get(func_id + 1) {
            if fun.addr_start() != funcs[func_id].addr_start() {
                break;
            }
            func_id += 1;
        }

        let mut fun = &funcs[func_id];

        // the binsearch might miss the function
        while !fun.addr_in_range(addr) {
            if let Some(parent_id) = fun.parent(func_id) {
                fun = &funcs[parent_id];
                func_id = parent_id;
            } else {
                // we missed entirely :(
                return Ok(vec![]);
            }
        }

        let mut rv = vec![];

        // what we hit directly
        rv.push(self.build_symbol(&fun, addr, None)?);

        // inlined outer parts
        while let Some(parent_id) = fun.parent(func_id) {
            let outer_addr = fun.addr_start();
            fun = &funcs[parent_id];
            func_id = parent_id;
            let symbol = {
                self.build_symbol(&fun, outer_addr, Some(&rv[rv.len() - 1]))?
            };
            rv.push(symbol);
        }

        Ok(rv)
    }
}

impl<'a> fmt::Debug for SymCache<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("SymCache")
            .field("size", &self.size())
            .field("arch", &self.arch().unwrap_or(Arch::Unknown))
            .field("data_source", &self.data_source().unwrap_or(DataSource::Unknown))
            .field("has_line_info", &self.has_line_info().unwrap_or(false))
            .field("has_file_info", &self.has_file_info().unwrap_or(false))
            .field("functions", &self.function_records().map(|x| x.len()).unwrap_or(0))
            .finish()
    }
}
