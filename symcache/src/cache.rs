use std::io;
use std::mem;
use std::str;
use std::fmt;
use std::ffi::CStr;

use symbolic_common::{Result, ErrorKind, ByteView};
use symbolic_debuginfo::Object;

use types::{CacheFileHeader, Seg, FileRecord, FuncRecord, LineRecord};
use utils::binsearch_by_key;
use writer::write_sym_cache;

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
    pub fn arch(&self) -> &str {
        self.cache.arch().unwrap_or("unknown")
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

impl<'a> SymCache<'a> {

    /// Load a symcache from a byteview.
    pub fn new(byteview: ByteView<'a>) -> Result<SymCache<'a>> {
        let rv = SymCache {
            byteview: byteview,
        };
        {
            let header = rv.header()?;
            if header.version != 2 {
                return Err(ErrorKind::UnknownCacheFileVersion(
                    header.version).into());
            }
        }
        Ok(rv)
    }

    /// Constructs a symcache from an object.
    pub fn from_object(obj: &Object) -> Result<SymCache<'a>> {
        let mut out: Vec<u8> = vec![];
        write_sym_cache(io::Cursor::new(out), obj)?;
        panic!("this is not implemented yet");
        //SymCache::new(ByteView::from_vec(out))
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
        let offset = seg.offset as usize + mem::size_of::<CacheFileHeader>();
        let size = mem::size_of::<T>() * seg.len as usize;
        unsafe {
            Ok(mem::transmute(self.get_data(offset, size)?))
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
    pub fn arch(&self) -> Result<&str> {
        let header = self.header()?;
        unsafe {
            Ok(CStr::from_ptr(header.arch.as_ptr()).to_str()?)
        }
    }

    /// Looks up a single symbol
    fn get_symbol(&self, idx: u32) -> Result<Option<&str>> {
        let header = self.header()?;
        let syms = self.get_segment(&header.symbols)?;
        if let Some(ref seg) = syms.get(idx as usize) {
            Ok(Some(self.get_segment_as_string(seg)?))
        } else {
            Ok(None)
        }
    }

    fn functions(&'a self) -> Result<&'a [FuncRecord]> {
        let header = self.header()?;
        self.get_segment(&header.function_records)
    }

    fn line_records(&'a self) -> Result<&'a [Seg<LineRecord>]> {
        let header = self.header()?;
        self.get_segment(&header.line_records)
    }

    fn run_to_line(&'a self, fun: &'a FuncRecord, addr: u64) -> Result<(&FileRecord, u32)> {
        let records_seg = match self.line_records()?.get(fun.line_record_id as usize) {
            Some(records) => records,
            None => { return Err(ErrorKind::Internal("unknown line record").into()) }
        };
        let records = self.get_segment(records_seg)?;

        let mut file_id = !0u16;
        let mut line = 0u32;
        let mut running_addr = fun.line_start as u64;

        for rec in records {
            let new_instr = running_addr + rec.addr_off as u64;
            let new_line = line + rec.line as u32;
            if new_instr >= addr {
                break;
            }
            running_addr = new_instr;
            line = new_line;
            file_id = rec.file_id;
        }

        let header = self.header()?;
        let files = self.get_segment(&header.files)?;

        if let Some(ref record) = files.get(file_id as usize) {
            Ok((record, line))
        } else {
            Err(ErrorKind::Internal("unknown file id").into())
        }
    }

    fn build_symbol(&'a self, fun: &'a FuncRecord, addr: u64) -> Result<Symbol<'a>> {
        let (file_record, line) = self.run_to_line(fun, addr)?;
        Ok(Symbol {
            cache: self,
            sym_addr: fun.addr_start(),
            instr_addr: addr,
            line: line,
            symbol: self.get_symbol(fun.symbol_id)?,
            filename: self.get_segment_as_string(&file_record.filename)?,
            comp_dir: self.get_segment_as_string(&file_record.comp_dir)?,
        })
    }

    /// Given an address this looks up the symbol at that point.
    ///
    /// Because of inling information this returns a vector of zero or
    /// more symbols.  If nothing is found then the return value will be
    /// an empty vector.
    pub fn lookup(&'a self, addr: u64) -> Result<Vec<Symbol<'a>>> {
        let funcs = self.functions()?;
        let mut fun = match binsearch_by_key(funcs, addr, |x| x.addr_start()) {
            Some(fun) => fun,
            None => { return Ok(vec![]); }
        };

        // the binsearch might mis the function
        while !fun.addr_in_range(addr) {
            if let Some(parent_id) = fun.get_parent_func() {
                fun = &funcs[parent_id];
            } else {
                // we missed entirely :(
                return Ok(vec![]);
            }
        }

        let mut rv = vec![];

        // what we hit directly
        rv.push(self.build_symbol(&fun, addr)?);

        // inlined outer parts
        while let Some(parent_id) = fun.get_parent_func() {
            let outer_addr = fun.addr_start();
            fun = &funcs[parent_id];
            rv.push(self.build_symbol(&fun, outer_addr)?);
        }

        Ok(rv)
    }
}
