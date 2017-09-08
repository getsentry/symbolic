use std::mem;
use std::str;
use std::slice;
use std::borrow::Cow;
use std::path::Path;
use std::ffi::CStr;

use memmap::{Mmap, Protection};

use symbolic_common::{Result, ErrorKind};

use types::{CacheFileHeader, Seg, FileRecord, FuncRecord, LineRecord};
use utils::binsearch_by_key;

pub struct Symbol<'a> {
    cache: &'a SymCache<'a>,
    sym_addr: u64,
    instr_addr: u64,
    line: u32,
    filename: &'a str,
    comp_dir: &'a str,
}

enum Backing<'a> {
    Buf(Cow<'a, [u8]>),
    Mmap(Mmap),
}

pub struct SymCache<'a> {
    backing: Backing<'a>,
}

impl<'a> Symbol<'a> {
    pub fn sym_addr(&self) -> u64 {
        self.sym_addr
    }

    pub fn instr_addr(&self) -> u64 {
        self.instr_addr
    }

    pub fn line(&self) -> u32 {
        self.line
    }

    pub fn filename(&self) -> &str {
        self.filename
    }

    pub fn comp_dir(&self) -> &str {
        self.comp_dir
    }
}

impl<'a> Backing<'a> {

    fn get_data(&self, start: usize, len: usize) -> Result<&[u8]> {
        let buffer = self.buffer();
        let end = start.wrapping_add(len);
        if end < start || end > buffer.len() {
            Err(ErrorKind::CorruptCacheFile.into())
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
            Ok(mem::transmute(self.get_data(0, mem::size_of::<CacheFileHeader>())?.as_ptr()))
        }
    }

    #[inline(always)]
    fn buffer(&self) -> &[u8] {
        match *self {
            Backing::Buf(ref buf) => buf,
            Backing::Mmap(ref mmap) => unsafe { mmap.as_slice() }
        }
    }
}

fn load_cachefile<'a>(backing: Backing<'a>) -> Result<SymCache<'a>> {
    {
        let header = backing.header()?;
        if header.version != 2 {
            return Err(ErrorKind::UnknownCacheFileVersion(header.version).into());
        }
    }
    Ok(SymCache {
        backing: backing,
    })
}

impl<'a> SymCache<'a> {

    /// Constructs a memdb object from a byte slice cow.
    pub fn from_cow(cow: Cow<'a, [u8]>) -> Result<SymCache<'a>> {
        load_cachefile(Backing::Buf(cow))
    }

    /// Constructs a memdb object from a byte slice.
    pub fn from_slice(buffer: &'a [u8]) -> Result<SymCache<'a>> {
        SymCache::from_cow(Cow::Borrowed(buffer))
    }

    /// Constructs a memdb object from a byte vector.
    pub fn from_vec(buffer: Vec<u8>) -> Result<SymCache<'a>> {
        SymCache::from_cow(Cow::Owned(buffer))
    }

    /// Constructs a memdb object by mmapping a file from the filesystem in.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<SymCache<'a>> {
        let mmap = Mmap::open_path(path, Protection::Read)?;
        load_cachefile(Backing::Mmap(mmap))
    }

    /// The architecture of the cache file
    pub fn arch(&self) -> Result<&str> {
        let header = self.backing.header()?;
        unsafe {
            Ok(CStr::from_ptr(header.arch.as_ptr()).to_str()?)
        }
    }

    /// Looks up a single symbol
    fn get_symbol(&self, idx: u32) -> Result<Option<&str>> {
        let header = self.backing.header()?;
        let syms = self.backing.get_segment(&header.symbols)?;
        if let Some(ref seg) = syms.get(idx as usize) {
            Ok(Some(self.backing.get_segment_as_string(seg)?))
        } else {
            Ok(None)
        }
    }

    fn functions(&'a self) -> Result<&'a [FuncRecord]> {
        let header = self.backing.header()?;
        self.backing.get_segment(&header.function_index)
    }

    fn line_records(&'a self) -> Result<&'a [Seg<LineRecord>]> {
        let header = self.backing.header()?;
        self.backing.get_segment(&header.line_records)
    }

    fn run_to_line(&'a self, fun: &'a FuncRecord, addr: u64) -> Result<(&FileRecord, u32)> {
        let header = self.backing.header()?;
        let records_seg = match self.line_records()?.get(fun.line_record_id as usize) {
            Some(records) => records,
            None => { return Err(ErrorKind::InternalError("unknown line record").into()) }
        };
        let records = self.backing.get_segment(records_seg)?;

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

        let header = self.backing.header()?;
        let files = self.backing.get_segment(&header.files)?;

        if let Some(ref record) = files.get(file_id as usize) {
            Ok((record, line))
        } else {
            Err(ErrorKind::InternalError("unknown file id").into())
        }
    }

    fn build_symbol(&'a self, fun: &'a FuncRecord, addr: u64) -> Result<Symbol<'a>> {
        let (file_record, line) = self.run_to_line(fun, addr)?;
        Ok(Symbol {
            cache: self,
            sym_addr: fun.addr_start(),
            instr_addr: addr,
            line: line,
            filename: self.backing.get_segment_as_string(&file_record.filename)?,
            comp_dir: self.backing.get_segment_as_string(&file_record.comp_dir)?,
        })
    }

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
