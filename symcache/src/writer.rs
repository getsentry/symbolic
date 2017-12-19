use std::cell::RefCell;
use std::collections::HashMap;
use std::cmp;
use std::io::{Seek, SeekFrom, Write};
use std::iter::Peekable;
use std::mem;
use std::slice;

use fnv::{FnvHashMap, FnvHashSet};
use num;

use symbolic_common::{DebugKind, Error, ErrorKind, Language, Result, ResultExt};
use symbolic_debuginfo::{Object, SymbolIterator, SymbolTable, Symbols};

use breakpad::BreakpadInfo;
use cache::SYMCACHE_MAGIC;
use dwarf::{DwarfInfo, Function, Unit};
use types::{CacheFileHeader, DataSource, FileRecord, FuncRecord, LineRecord, Seg};

fn err(msg: &'static str) -> Error {
    Error::from(ErrorKind::BadDwarfData(msg))
}

/// Given a writer and object, dumps the object into the writer.
///
/// In case a symcache is to be constructed from memory the `SymCache::from_object`
/// method can be used instead.
///
/// This requires the writer to be seekable.
pub fn to_writer<W: Write + Seek>(mut w: W, obj: &Object) -> Result<()> {
    w.write_all(CacheFileHeader::default().as_bytes())?;
    let header = {
        let mut writer = SymCacheWriter::new(&mut w);
        writer.write_object(obj)?;
        writer.header
    };
    w.seek(SeekFrom::Start(0))?;
    w.write_all(header.as_bytes())?;
    Ok(())
}

/// Converts an object into a vector of symcache data.
pub fn to_vec(obj: &Object) -> Result<Vec<u8>> {
    let mut buf = Vec::<u8>::new();
    buf.write_all(CacheFileHeader::default().as_bytes())?;
    let header = {
        let mut writer = SymCacheWriter::new(&mut buf);
        writer.write_object(obj)?;
        writer.header
    };
    let header_bytes = header.as_bytes();
    (&mut buf[..header_bytes.len()]).copy_from_slice(header_bytes);
    Ok(buf)
}

#[derive(Debug)]
enum DebugInfo<'input> {
    Dwarf(DwarfInfo<'input>),
    Breakpad(BreakpadInfo<'input>),
}

impl<'input> DebugInfo<'input> {
    pub fn from_object(object: &'input Object) -> Result<DebugInfo<'input>> {
        Ok(match object.debug_kind() {
            Some(DebugKind::Dwarf) => DebugInfo::Dwarf(DwarfInfo::from_object(object)?),
            Some(DebugKind::Breakpad) => DebugInfo::Breakpad(BreakpadInfo::from_object(object)?),
            // Add this when more object kinds are added in symbolic_debuginfo:
            // Some(_) => return Err(ErrorKind::UnsupportedObjectFile.into()),
            None => {
                return Err(
                    ErrorKind::MissingDebugInfo("symcache only supports DWARF and Breakpad").into(),
                )
            }
        })
    }
}

struct SymCacheWriter<W: Write> {
    writer: RefCell<(u64, W)>,
    header: CacheFileHeader,
    symbol_map: HashMap<Vec<u8>, u32>,
    symbols: Vec<Seg<u8, u16>>,
    files: HashMap<Vec<u8>, Seg<u8, u8>>,
    file_record_map: HashMap<FileRecord, u16>,
    file_records: Vec<FileRecord>,
    func_records: Vec<FuncRecord>,
    line_record_bytes: RefCell<u64>,
}

impl<W: Write> SymCacheWriter<W> {
    pub fn new(writer: W) -> SymCacheWriter<W> {
        SymCacheWriter {
            writer: RefCell::new((0, writer)),
            header: Default::default(),
            symbol_map: HashMap::new(),
            symbols: vec![],
            files: HashMap::new(),
            file_record_map: HashMap::new(),
            file_records: vec![],
            func_records: vec![],
            line_record_bytes: RefCell::new(0),
        }
    }

    #[inline(always)]
    fn write_bytes<L>(&self, bytes: &[u8]) -> Result<Seg<u8, L>>
    where
        L: Copy + num::FromPrimitive,
    {
        let (ref mut pos, ref mut writer) = *self.writer.borrow_mut();
        let offset = *pos;
        *pos += bytes.len() as u64;
        writer.write_all(bytes)?;
        Ok(Seg::new(
            offset as u32,
            num::FromPrimitive::from_usize(bytes.len())
                .ok_or_else(|| ErrorKind::Internal("out of range for byte segment"))?,
        ))
    }

    #[inline(always)]
    fn write_item<T, L>(&self, x: &T) -> Result<Seg<u8, L>>
    where
        L: Copy + num::FromPrimitive,
    {
        unsafe {
            let bytes: *const u8 = mem::transmute(x);
            let size = mem::size_of_val(x);
            self.write_bytes(slice::from_raw_parts(bytes, size))
        }
    }

    #[inline]
    fn write_seg<T, L>(&self, x: &[T]) -> Result<Seg<T, L>>
    where
        L: Copy + num::FromPrimitive,
    {
        let mut first_seg: Option<Seg<u8>> = None;
        for item in x {
            let seg = self.write_item(item)?;
            if first_seg.is_none() {
                first_seg = Some(seg);
            }
        }
        Ok(Seg::new(
            first_seg.map(|x| x.offset).unwrap_or(0),
            num::FromPrimitive::from_usize(x.len())
                .ok_or_else(|| ErrorKind::BadDwarfData("out of range for item segment"))?,
        ))
    }

    fn write_symbol_if_missing(&mut self, sym: &[u8]) -> Result<u32> {
        if let Some(&index) = self.symbol_map.get(sym) {
            return Ok(index);
        }
        if self.symbols.len() >= 0xffffff {
            return Err(ErrorKind::BadDwarfData("Too many symbols").into());
        }
        let idx = self.symbols.len() as u32;
        let seg = self.write_bytes(sym)?;
        self.symbols.push(seg);
        self.symbol_map.insert(sym.to_owned(), idx);
        Ok(idx)
    }

    #[inline]
    fn write_file_if_missing(&mut self, filename: &[u8]) -> Result<Seg<u8, u8>> {
        if let Some(item) = self.files.get(filename) {
            return Ok(*item);
        }
        let seg = self.write_bytes(filename)?;
        self.files.insert(filename.to_owned(), seg);
        Ok(seg)
    }

    fn write_file_record_if_missing(&mut self, record: FileRecord) -> Result<u16> {
        if let Some(idx) = self.file_record_map.get(&record) {
            return Ok(*idx);
        }
        if self.file_records.len() >= 0xffff {
            return Err(ErrorKind::BadDwarfData("Too many symbols").into());
        }
        let idx = self.file_records.len() as u16;
        self.file_record_map.insert(record, idx);
        self.file_records.push(record);
        Ok(idx)
    }

    pub fn write_object(&mut self, obj: &Object) -> Result<()> {
        // common header values
        self.header.magic = SYMCACHE_MAGIC;
        self.header.version = 1;
        self.header.arch = obj.arch() as u32;
        if let Some(uuid) = obj.uuid() {
            self.header.uuid = uuid;
        }

        // try dwarf data first.  If we cannot find the necessary dwarf sections
        // we just skip over to symbol table processing.
        match DebugInfo::from_object(obj) {
            Ok(DebugInfo::Dwarf(ref info)) => {
                return self.write_dwarf_info(info, obj.symbols().ok())
                    .chain_err(|| err("could not process DWARF data"));
            }
            Ok(DebugInfo::Breakpad(ref info)) => {
                return self.write_breakpad_info(info)
                    .chain_err(|| err("could not process Breakpad symbols"));
            }
            Err(Error(ErrorKind::MissingSection(..), ..)) => {
                // ignore missing sections
            }
            Err(e) => {
                return Err(e).chain_err(|| err("could not load debug data"))?;
            }
        }

        // fallback to symbol table.
        match obj.symbols() {
            Ok(symbols) => {
                return self.write_symbol_table(symbols.iter(), obj.vmaddr()?)
                    .chain_err(|| err("Could not process symbol table"));
            }
            Err(Error(ErrorKind::MissingDebugInfo(..), ..)) => {
                // ignore missing debug info
            }
            Err(e) => {
                return Err(e).chain_err(|| err("could not load symnbol table"));
            }
        }

        Err(ErrorKind::MissingDebugInfo("No debug info found in file").into())
    }

    fn write_symbol_table(&mut self, symbols: SymbolIterator, vmaddr: u64) -> Result<()> {
        for symbol_result in symbols {
            let func = symbol_result?;
            self.write_simple_function(func.addr - vmaddr, func.len.unwrap_or(!0), func.name)?;
        }

        self.header.data_source = DataSource::SymbolTable as u8;
        self.header.symbols = self.write_seg(&self.symbols)?;
        self.header.function_records = self.write_seg(&self.func_records)?;

        Ok(())
    }

    fn write_missing_functions_from_symboltable(
        &mut self,
        last_addr: &mut u64,
        cur_addr: u64,
        vmaddr: u64,
        symbol_iter: &mut Peekable<SymbolIterator>,
    ) -> Result<()> {
        while let Some(&Ok(symbol)) = symbol_iter.peek() {
            let sym_addr = symbol.addr - vmaddr;
            let sym_len = symbol.len.unwrap_or(!0);

            // skip forward until we hit a relevant symbol
            if *last_addr != !0 && sym_addr < *last_addr {
                symbol_iter.next();
                continue;
            }

            if (*last_addr == !0 || sym_addr >= *last_addr) && sym_addr < cur_addr {
                self.write_simple_function(sym_addr, sym_len, symbol.name)?;
                symbol_iter.next();
                *last_addr = sym_addr + sym_len;
            } else {
                break;
            }
        }
        Ok(())
    }

    fn write_simple_function<S>(&mut self, func_addr: u64, len: u64, symbol: S) -> Result<()>
    where
        S: AsRef<[u8]>,
    {
        let symbol_id = self.write_symbol_if_missing(symbol.as_ref())?;

        self.func_records.push(FuncRecord {
            addr_low: (func_addr & 0xffffffff) as u32,
            addr_high: ((func_addr >> 32) & 0xffff) as u16,
            // XXX: we have not seen this yet, but in theory this should be
            // stored as multiple function records.
            len: cmp::min(len, 0xffff) as u16,
            symbol_id_low: (symbol_id & 0xffff) as u16,
            symbol_id_high: ((symbol_id >> 16) & 0xff) as u8,
            parent_offset: !0,
            line_records: Seg::default(),
            comp_dir: Seg::default(),
            lang: Language::Unknown as u8,
        });
        Ok(())
    }

    fn write_breakpad_info(&mut self, info: &BreakpadInfo) -> Result<()> {
        let mut file_cache = FnvHashMap::default();

        for file in info.files() {
            if file_cache.contains_key(&file.id) {
                continue;
            }

            let file_record = FileRecord {
                filename: self.write_file_if_missing(file.name)?,
                base_dir: self.write_file_if_missing(b"")?,
            };

            let file_id = self.write_file_record_if_missing(file_record)?;
            file_cache.insert(&file.id, file_id);
        }

        let mut syms = info.symbols().iter().peekable();
        for function in info.functions() {
            // Write all symbols that are not defined in info.functions()
            while syms.peek().map_or(false, |s| s.address < function.address) {
                let symbol = syms.next().unwrap();
                self.write_simple_function(symbol.address, symbol.size, symbol.name)?;
            }

            // Skip symbols that are also defined in info.functions()
            let next_address = function.address + cmp::max(function.size, 1);
            while syms.peek().map_or(false, |s| s.address < next_address) {
                syms.next();
            }

            let func_id = self.func_records.len();
            self.write_simple_function(function.address, function.size, function.name)?;

            if function.lines.is_empty() {
                continue;
            }

            let mut line_records = vec![];
            let mut last_addr = function.address;
            for line in &function.lines {
                let mut diff = line.address.saturating_sub(last_addr) as i64;
                last_addr += diff as u64;

                while diff >= 0 {
                    let file_id = match file_cache.get(&line.file_id) {
                        Some(id) => *id,
                        None => return Err(ErrorKind::BadBreakpadSym("Invalid file_id").into()),
                    };

                    line_records.push(LineRecord {
                        addr_off: (diff & 0xff) as u8,
                        file_id: file_id,
                        line: cmp::min(line.line, 0xffff) as u16,
                    });

                    diff -= 0xff;
                }
            }

            self.func_records[func_id].line_records = self.write_seg(&line_records)?;
            self.header.has_line_records = 1;
        }

        // Flush out all remaining symbols from the symbol table (PUBLIC records)
        for symbol in syms {
            self.write_simple_function(symbol.address, symbol.size, symbol.name)?;
        }

        self.header.data_source = DataSource::BreakpadSym as u8;
        self.header.symbols = self.write_seg(&self.symbols)?;
        self.header.files = self.write_seg(&self.file_records)?;
        self.header.function_records = self.write_seg(&self.func_records)?;

        Ok(())
    }

    fn write_dwarf_info(&mut self, info: &DwarfInfo, symbols: Option<Symbols>) -> Result<()> {
        let symbols = symbols.as_ref();

        let mut range_buf = Vec::new();
        let mut symbol_iter = symbols.map(|x| x.iter().peekable());
        let mut last_addr = !0;
        let mut addrs = FnvHashSet::default();
        let mut local_cache = FnvHashMap::default();
        let mut funcs = vec![];

        for index in 0..info.units.len() {
            // attempt to parse a single unit from the given header.
            let unit_opt = Unit::parse(&info, index)
                .chain_err(|| err("encountered invalid compilation unit"))?;

            // skip units we don't care about
            let unit = match unit_opt {
                Some(unit) => unit,
                None => continue,
            };

            // clear our function local caches and infos
            let addrs_inner = &mut addrs;
            let local_cache_inner = &mut local_cache;
            addrs_inner.clear();
            local_cache_inner.clear();
            funcs.clear();

            unit.get_functions(&info, &mut range_buf, symbols, &mut funcs)?;
            for func in &funcs {
                // dedup instructions from inline functions
                if let &mut Some(ref mut symbol_iter) = &mut symbol_iter {
                    self.write_missing_functions_from_symboltable(
                        &mut last_addr,
                        func.addr,
                        info.vmaddr,
                        symbol_iter,
                    )?;
                }
                self.write_dwarf_function(&func, addrs_inner, local_cache_inner, !0)?;
                last_addr = func.addr + func.len as u64;
            }
        }

        if let &mut Some(ref mut symbol_iter) = &mut symbol_iter {
            self.write_missing_functions_from_symboltable(
                &mut last_addr,
                !0,
                info.vmaddr,
                symbol_iter,
            )?;
        }

        self.header.data_source = DataSource::Dwarf as u8;
        self.header.symbols = self.write_seg(&self.symbols)?;
        self.header.files = self.write_seg(&self.file_records)?;
        self.header.function_records = self.write_seg(&self.func_records)?;

        Ok(())
    }

    fn write_dwarf_function<'a>(
        &mut self,
        func: &Function<'a>,
        addrs: &mut FnvHashSet<u64>,
        local_cache: &mut FnvHashMap<u64, u16>,
        parent_id: u32,
    ) -> Result<()> {
        // if we have a function without any instructions we just skip it.  This
        // saves memory and since we only care about instructions where we can
        // actually crash this is a reasonable optimization.
        if func.is_empty() {
            return Ok(());
        }

        let func_id = self.func_records.len() as u32;
        let func_addr = func.get_addr();

        let symbol_id = self.write_symbol_if_missing(func.name)?;
        let func_record = FuncRecord {
            addr_low: (func_addr & 0xffffffff) as u32,
            addr_high: ((func_addr >> 32) & 0xffff) as u16,
            // XXX: we have not seen this yet, but in theory this should be
            // stored as multiple function records.
            len: cmp::min(func.len, 0xffff) as u16,
            symbol_id_low: (symbol_id & 0xffff) as u16,
            symbol_id_high: ((symbol_id >> 16) & 0xff) as u8,
            parent_offset: if parent_id == !0 {
                !0
            } else {
                let parent_offset = func_id.saturating_sub(parent_id);
                if parent_offset == !0 {
                    return Err(ErrorKind::Internal(
                        "parent function range too big for file format",
                    ).into());
                }
                parent_offset as u16
            },
            line_records: Seg::default(),
            comp_dir: self.write_file_if_missing(func.comp_dir)?,
            lang: if func.lang as u32 > 0xff {
                return Err(ErrorKind::Internal("language out of range for file format").into());
            } else {
                func.lang as u8
            },
        };
        let mut last_addr = func_record.addr_start();
        self.func_records.push(func_record);

        // recurse first.  As we recurse down the address rejection will
        // do the job it's supposed to do.
        for inline_func in &func.inlines {
            self.write_dwarf_function(inline_func, addrs, local_cache, func_id)?;
        }

        let mut line_records = vec![];
        for line in &func.lines {
            if addrs.contains(&line.addr) {
                continue;
            }
            addrs.insert(line.addr);

            let file_id = if let Some(&x) = local_cache.get(&line.original_file_id) {
                x
            } else {
                let file_record = FileRecord {
                    filename: self.write_file_if_missing(line.filename)?,
                    base_dir: self.write_file_if_missing(line.base_dir)?,
                };
                let file_id = self.write_file_record_if_missing(file_record)?;
                local_cache.insert(line.original_file_id, file_id);
                file_id
            };

            // We have seen that swift can generate line records that lie outside
            // of the function start.  Why this happens is unclear but it happens
            // with highly inlined function calls.  Instead of panicking we want
            // to just assume there is a single record at the address of the function
            // and in case there are more the offsets are just slightly off.
            let mut diff = (line.addr.saturating_sub(last_addr)) as i64;

            while diff >= 0 {
                let line_record = LineRecord {
                    addr_off: (diff & 0xff) as u8,
                    file_id: file_id,
                    line: line.line,
                };
                last_addr += line_record.addr_off as u64;
                line_records.push(line_record);
                diff -= 0xff;
            }

            let mut counter = self.line_record_bytes.borrow_mut();
            *counter += mem::size_of::<LineRecord>() as u64;
        }

        if !line_records.is_empty() {
            self.func_records[func_id as usize].line_records = self.write_seg(&line_records)?;
            self.header.has_line_records = 1;
        }

        Ok(())
    }
}
