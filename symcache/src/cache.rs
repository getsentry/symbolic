use std::io;
use std::mem;
use std::str;
use std::fmt;
use std::slice;
use std::cell::RefCell;

use symbolic_common::{Result, ErrorKind, ByteView, Arch, Language};
use symbolic_debuginfo::Object;

use types::{CacheFileHeader, Seg, FileRecord, FuncRecord, LineRecord};
use utils::binsearch_by_key;
use writer;

pub const SYMCACHE_MAGIC: [u8; 4] = [b'S', b'Y', b'M', b'C'];

/// A matched symbol
pub struct Symbol<'a> {
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

/// An abstraction around a symbol cache file.
pub struct SymCache<'a> {
    byteview: ByteView<'a>,
}

impl<'a> Symbol<'a> {
    /// The architecture of the matched symbol.
    pub fn arch(&self) -> Arch {
        self.cache.arch().unwrap_or(Arch::Unknown)
    }

    /// The address where the symbol starts.
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

    /// The current language.
    pub fn lang(&self) -> Language {
        self.lang
    }

    /// The string value of the symbol (mangled).
    pub fn symbol(&self) -> &str {
        self.symbol.unwrap_or("?")
    }

    /// The filename of the current line.
    pub fn filename(&self) -> &str {
        self.filename
    }

    /// The base dir of the current line.
    pub fn base_dir(&self) -> &str {
        self.base_dir
    }

    /// The compilation dir of the function.
    pub fn comp_dir(&self) -> &str {
        self.comp_dir
    }
}

impl<'a> fmt::Display for Symbol<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}\n", self.symbol())?;
        write!(f, "  at {}/{} line {}", self.base_dir(), self.filename(), self.line())?;
        Ok(())
    }
}

impl<'a> fmt::Debug for Symbol<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Symbol")
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
        self.cache.get_symbol(self.fun.symbol_id).unwrap_or(None).unwrap_or("")
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

macro_rules! itry {
    ($expr:expr) => {
        match $expr {
            Ok(rv) => rv,
            Err(err) => {
                return Some(Err(::std::convert::From::from(err)));
            }
        }
    }
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

    fn get_segment<T>(&self, seg: &Seg<T>) -> Result<&[T]> {
        let offset = seg.offset as usize +
            mem::size_of::<CacheFileHeader>() as usize;
        let size = mem::size_of::<T>() * seg.len as usize;
        unsafe {
            let bytes = self.get_data(offset, size)?;
            Ok(slice::from_raw_parts(
                mem::transmute(bytes.as_ptr()),
                seg.len as usize
            ))
        }
    }

    fn get_segment_as_string(&self, seg: &Seg<u8>) -> Result<&str> {
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
        let header = self.header()?;
        let files = self.get_segment(&header.files)?;
        Ok(files.get(idx as usize))
    }

    fn run_to_line(&'a self, fun: &'a FuncRecord, addr: u64)
        -> Result<Option<(&FileRecord, u32)>>
    {
        let records = self.get_segment(&fun.line_records)?;

        let mut file_id = !0u16;
        let mut running_addr = fun.addr_start() as u64;
        let mut line = 0u32;

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
                    inner_sym: Option<&Symbol<'a>>) -> Result<Symbol<'a>> {
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
        Ok(Symbol {
            cache: self,
            sym_addr: fun.addr_start(),
            instr_addr: addr,
            line: line,
            lang: Language::from_u32(fun.lang as u32).unwrap_or(Language::Unknown),
            symbol: self.get_symbol(fun.symbol_id)?,
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
    pub fn lookup(&'a self, addr: u64) -> Result<Vec<Symbol<'a>>> {
        let funcs = self.function_records()?;

        // functions in the function segment are ordered by start address
        // primarily and by depth secondarily.  As a result we want to have
        // a secondary comparison by the item index.
        let (mut func_id, mut fun) = match binsearch_by_key(
            funcs, (addr, !0), |idx, rec| (rec.addr_start(), idx))
        {
            Some(item) => item,
            None => { return Ok(vec![]); }
        };

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
