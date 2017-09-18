use std::io;
use std::mem;
use std::str;
use std::fmt;
use std::slice;

use symbolic_common::{Result, ErrorKind, ByteView, Arch};
use symbolic_debuginfo::Object;

use types::{CacheFileHeader, Seg, FileRecord, FuncRecord, LineRecord};
use utils::binsearch_by_key;
use writer::write_symcache;

/// A matched symbol
pub struct Symbol<'a> {
    cache: &'a SymCache<'a>,
    sym_addr: u64,
    instr_addr: u64,
    line: u32,
    symbol: Option<&'a str>,
    filename: &'a str,
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

    /// The string value of the symbol (mangled).
    pub fn symbol(&self) -> &str {
        self.symbol.unwrap_or("?")
    }

    /// The filename of the current line.
    pub fn filename(&self) -> &str {
        self.filename
    }

    /// The compilation dir of the current line.
    pub fn comp_dir(&self) -> &str {
        self.comp_dir
    }
}

impl<'a> fmt::Display for Symbol<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}\n", self.symbol())?;
        write!(f, "  at {}/{} line {}", self.comp_dir(), self.filename(), self.line())?;
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
            .field("comp_dir", &self.comp_dir())
            .finish()
    }
}

/// A view of a single function in a sym cache.
pub struct Function<'a> {
    symbol: Option<&'a str>,
    addr: u64,
}

impl<'a> Function<'a> {
    pub fn addr(&self) -> u64 {
        self.addr
    }

    pub fn symbol(&self) -> &str {
        self.symbol.unwrap_or("")
    }
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
                symbol: itry!(self.cache.get_symbol(fun.symbol_id)),
                addr: fun.addr_start(),
            }))
        } else {
            None
        }
    }
}

impl<'a> fmt::Debug for Function<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Function")
            .field("symbol", &self.symbol())
            .field("addr", &self.addr())
            .finish()
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
            if header.version != 1 {
                return Err(ErrorKind::UnknownCacheFileVersion(
                    header.version).into());
            }
        }
        Ok(rv)
    }

    /// Constructs a symcache from an object.
    pub fn from_object(obj: &Object) -> Result<SymCache<'a>> {
        let mut cur = io::Cursor::new(Vec::<u8>::new());
        write_symcache(&mut cur, obj)?;
        SymCache::new(ByteView::from_vec(cur.into_inner()))
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
        let offset = seg.offset as usize;
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

    fn get_line_records(&'a self, id: u32) -> Result<Option<&'a [LineRecord]>> {
        if id == !0 {
            return Ok(None);
        }
        let header = self.header()?;
        let records = self.get_segment(&header.line_records)?;
        match records.get(id as usize) {
            Some(records_seg) => Ok(Some(self.get_segment(records_seg)?)),
            None => Err(ErrorKind::Internal("unknown line record").into())
        }
    }

    fn run_to_line(&'a self, fun: &'a FuncRecord, addr: u64)
        -> Result<Option<(&FileRecord, u32)>>
    {
        let records = match self.get_line_records(fun.line_record_id)? {
            Some(records) => records,
            None => { return Ok(None); }
        };

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

        let header = self.header()?;
        let files = self.get_segment(&header.files)?;

        if let Some(ref record) = files.get(file_id as usize) {
            Ok(Some((record, line)))
        } else {
            Err(ErrorKind::Internal("unknown file id").into())
        }
    }

    fn build_symbol(&'a self, fun: &'a FuncRecord, addr: u64,
                    inner_sym: Option<&Symbol<'a>>) -> Result<Symbol<'a>> {
        let (line, filename, comp_dir) = match self.run_to_line(fun, addr)? {
            Some((file_record, line)) => {
                (
                    line,
                    self.get_segment_as_string(&file_record.filename)?,
                    self.get_segment_as_string(&file_record.comp_dir)?,
                )
            }
            None => {
                if let Some(inner_sym) = inner_sym {
                    (inner_sym.line, inner_sym.filename, inner_sym.comp_dir)
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
            symbol: self.get_symbol(fun.symbol_id)?,
            filename: filename,
            comp_dir: comp_dir,
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
        let mut fun = match binsearch_by_key(
            funcs, (addr, !0), |idx, rec| (rec.addr_start(), idx))
        {
            Some((_, fun)) => fun,
            None => { return Ok(vec![]); }
        };

        // the binsearch might miss the function
        while !fun.addr_in_range(addr) {
            if let Some(parent_id) = fun.parent() {
                fun = &funcs[parent_id];
            } else {
                // we missed entirely :(
                return Ok(vec![]);
            }
        }

        let mut rv = vec![];

        // what we hit directly
        rv.push(self.build_symbol(&fun, addr, None)?);

        // inlined outer parts
        while let Some(parent_id) = fun.parent() {
            let outer_addr = fun.addr_start();
            fun = &funcs[parent_id];
            let symbol = {
                self.build_symbol(&fun, outer_addr, Some(&rv[rv.len() - 1]))?
            };
            rv.push(symbol);
        }

        Ok(rv)
    }
}
