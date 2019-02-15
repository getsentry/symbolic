use std::fmt;

use symbolic_common::{Arch, DebugId, Language, Name};

use crate::error::{SymCacheError, SymCacheErrorKind};
use crate::format;

/// A platform independent symbolication cache.
pub struct SymCache<'a> {
    header: format::Header,
    data: &'a [u8],
}

impl<'a> SymCache<'a> {
    /// Parses a SymCache from a binary buffer.
    pub fn parse(mut data: &'a [u8]) -> Result<Self, SymCacheError> {
        let header = format::Header::parse(data)?;

        // The first version of SymCaches used to store offsets relative to the end of the header. This
        // was corrected in version 2 to store file-absolute offsets.
        if header.preamble.version == 1 {
            let offset = std::mem::size_of::<format::HeaderV1>();
            if data.len() > offset {
                data = &data[offset..];
            } else {
                data = &[];
            }
        }

        Ok(SymCache { header, data })
    }

    /// The version of the SymCache file format.
    pub fn version(&self) -> u32 {
        self.header.preamble.version
    }

    /// The architecture of the symbol file.
    pub fn arch(&self) -> Arch {
        Arch::from_u32(self.header.arch)
    }

    /// The id of the cache file.
    pub fn id(&self) -> DebugId {
        self.header.id
    }

    /// Returns true if line information is included.
    pub fn has_line_info(&self) -> bool {
        self.header.has_line_records != 0
    }

    /// Returns true if file information is included.
    pub fn has_file_info(&self) -> bool {
        // See the writers: if there is file information, there are also lines.
        self.has_line_info()
    }

    /// Returns an iterator over all functions.
    pub fn functions(&self) -> Functions<'a> {
        Functions {
            functions: self.header.functions,
            symbols: self.header.symbols,
            files: self.header.files,
            data: self.data,
            index: 0,
        }
    }

    /// Given an address this looks up the symbol at that point.
    ///
    /// Because of inling information this returns a vector of zero or
    /// more symbols.  If nothing is found then the return value will be
    /// an empty vector.
    pub fn lookup(&self, addr: u64) -> Result<Lookup<'a, '_>, SymCacheError> {
        let funcs = self.function_records()?;

        // Functions in the function segment are ordered by start address
        // primarily and by depth secondarily.  As a result we want to have
        // a secondary comparison by the item index.
        let mut func_id = match funcs.binary_search_by_key(&addr, |x| x.addr_start()) {
            Ok(idx) => idx,
            Err(0) => return Ok(Lookup::empty(self)),
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
                return Ok(Lookup::empty(self));
            }
        }

        Ok(Lookup {
            cache: self,
            funcs,
            current: Some((addr, func_id, fun)),
            inner: None,
        })
    }

    /// Resolves the raw list of `FuncRecords` from the funcs segment.
    fn function_records(&self) -> Result<&'a [format::FuncRecord], SymCacheError> {
        self.header.functions.read(self.data)
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
        fun: &'a format::FuncRecord,
        addr: u64,
    ) -> Result<Option<(&format::FileRecord, u64, u32)>, SymCacheError> {
        let records = fun.line_records.read(self.data)?;
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

        if let Some(ref record) = read_file_record(self.data, self.header.files, file_id)? {
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
        &self,
        fun: &'a format::FuncRecord,
        addr: u64,
        inner_sym: Option<(u32, u64, &'a str, &'a str)>,
    ) -> Result<LineInfo<'a>, SymCacheError> {
        let (line, line_addr, filename, base_dir) =
            if let Some((file_record, line_addr, line)) = self.run_to_line(fun, addr)? {
                // The address was found in the function's line records, so use
                // it directly. This should is the default case for all valid
                // debugging information and the majority of all frames.
                (
                    line,
                    line_addr,
                    file_record.filename.read_str(self.data)?,
                    file_record.base_dir.read_str(self.data)?,
                )
            } else if let Some(inner_sym) = inner_sym {
                // The source line was not declared in this function. This
                // happens, if the function body consists of a single inlined
                // function call. Usually, the `SymCacheWriter` should emit a
                // synthetic line record in this case; but if debug symbols did
                // not provide sufficient information, we will still hit this
                // case. Use the inlined frame's source location as a
                // replacement to point somewhere useful.
                inner_sym
            } else {
                // We were unable to find any source code. This can happen for
                // synthetic functions, such as Swift method thunks. In that
                // case, we can only return empty line information. Also top-
                // level functions without line records pulled from the symbol
                // table will hit this branch.
                (0, 0, "", "")
            };

        Ok(LineInfo {
            arch: self.arch(),
            id: self.id(),
            sym_addr: fun.addr_start(),
            line_addr,
            instr_addr: addr,
            line,
            lang: Language::from_u32(fun.lang.into()),
            symbol: read_symbol(self.data, self.header.symbols, fun.symbol_id())?,
            filename,
            base_dir,
            comp_dir: fun.comp_dir.read_str(self.data)?,
        })
    }
}

impl fmt::Debug for SymCache<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SymCache")
            .field("id", &self.id())
            .field("arch", &self.arch())
            .field("has_line_info", &self.has_line_info())
            .field("has_file_info", &self.has_file_info())
            .field("functions", &self.function_records().unwrap_or(&[]).len())
            .finish()
    }
}

#[derive(Clone)]
pub struct Lookup<'a, 'c> {
    cache: &'c SymCache<'a>,
    funcs: &'a [format::FuncRecord],
    current: Option<(u64, usize, &'a format::FuncRecord)>,
    inner: Option<(u32, u64, &'a str, &'a str)>,
}

impl<'a, 'c> Lookup<'a, 'c> {
    fn empty(cache: &'c SymCache<'a>) -> Self {
        Lookup {
            cache,
            funcs: &[],
            current: None,
            inner: None,
        }
    }
}

impl<'a, 'c> Iterator for Lookup<'a, 'c> {
    type Item = Result<LineInfo<'a>, SymCacheError>;

    fn next(&mut self) -> Option<Self::Item> {
        let (addr, id, fun) = self.current?;
        let line_result = self.cache.build_line_info(fun, addr, None);

        self.current = fun
            .parent(id)
            .map(|parent_id| (fun.addr_start(), parent_id, &self.funcs[parent_id]));

        if let Ok(ref line_info) = line_result {
            self.inner = Some((
                line_info.line(),
                line_info.line_address(),
                line_info.filename(),
                line_info.compilation_dir(),
            ));
        }

        Some(line_result)
    }
}

impl fmt::Debug for Lookup<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut list = f.debug_list();
        for line in self.clone() {
            match line {
                Ok(line) => {
                    list.entry(&line);
                }
                Err(error) => {
                    return error.fmt(f);
                }
            }
        }
        list.finish()
    }
}

/// Information on a matched source line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineInfo<'a> {
    arch: Arch,
    id: DebugId,
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

impl<'a> LineInfo<'a> {
    /// Architecture of the image referenced by this line.
    pub fn arch(&self) -> Arch {
        self.arch
    }

    /// Debug identifier of the image referenced by this line.
    pub fn id(&self) -> DebugId {
        self.id
    }

    /// The instruction address where the enclosing function starts.
    pub fn function_address(&self) -> u64 {
        self.sym_addr
    }

    /// The instruction address where the line starts.
    pub fn line_address(&self) -> u64 {
        self.line_addr
    }

    /// The actual instruction address.
    pub fn instruction_address(&self) -> u64 {
        self.instr_addr
    }

    /// The compilation directory of the function.
    pub fn compilation_dir(&self) -> &'a str {
        self.comp_dir
    }

    /// The base dir of the current line.
    pub fn base_dir(&self) -> &str {
        self.base_dir
    }

    /// The filename of the current line.
    pub fn filename(&self) -> &'a str {
        self.filename
    }

    /// The fully joined path and file name.
    pub fn path(&self) -> String {
        symbolic_common::join_path(self.base_dir, self.filename)
    }

    /// The line number within the file.
    pub fn line(&self) -> u32 {
        self.line
    }

    /// The source code language.
    pub fn language(&self) -> Language {
        self.lang
    }

    /// The string value of the symbol (mangled).
    pub fn symbol(&self) -> &'a str {
        self.symbol.unwrap_or("?")
    }

    /// The name of the function suitable for demangling.
    ///
    /// Use `symbolic::demangle` for demangling this symbol.
    pub fn function_name(&self) -> Name {
        Name::with_language(self.symbol(), self.language())
    }
}

impl fmt::Display for LineInfo<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.function_name())?;
        if f.alternate() {
            let path = self.path();
            let line = self.line();
            let lang = self.language();
            if path != "" || line != 0 || lang != Language::Unknown {
                write!(f, "\n ")?;
                if path != "" {
                    write!(f, " at {}", path)?;
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

/// An iterator over all functions in a `SymCache`.
#[derive(Clone, Debug)]
pub struct Functions<'a> {
    functions: format::Seg<format::FuncRecord>,
    symbols: format::Seg<format::Seg<u8, u16>>,
    files: format::Seg<format::FileRecord, u16>,
    data: &'a [u8],
    index: u32,
}

impl<'a> Iterator for Functions<'a> {
    type Item = Result<Function<'a>, SymCacheError>;

    fn next(&mut self) -> Option<Self::Item> {
        let record = match self.functions.get(self.data, self.index) {
            Ok(Some(record)) => record,
            Ok(None) => return None,
            Err(error) => return Some(Err(error)),
        };

        let function = Some(Ok(Function {
            record,
            symbols: self.symbols,
            files: self.files,
            data: self.data,
            index: self.index,
        }));

        self.index += 1;
        function
    }
}

#[derive(Clone)]
pub struct Function<'a> {
    record: &'a format::FuncRecord,
    symbols: format::Seg<format::Seg<u8, u16>>,
    files: format::Seg<format::FileRecord, u16>,
    data: &'a [u8],
    index: u32,
}

impl<'a> Function<'a> {
    /// The ID of the function.
    pub fn id(&self) -> usize {
        self.index as usize
    }

    /// The ID of the parent function, if this function was inlined.
    pub fn parent_id(&self) -> Option<usize> {
        self.record.parent(self.id())
    }

    /// The address where the function starts.
    pub fn address(&self) -> u64 {
        self.record.addr_start()
    }

    /// The raw name of the function.
    pub fn symbol(&self) -> &'a str {
        read_symbol(self.data, self.symbols, self.record.symbol_id())
            .unwrap_or(None)
            .unwrap_or("?")
    }

    /// The language of the function.
    pub fn language(&self) -> Language {
        Language::from_u32(self.record.lang.into())
    }

    /// The name of the function suitable for demangling.
    ///
    /// Use `symbolic::demangle` for demangling this symbol.
    pub fn name(&self) -> Name {
        Name::with_language(self.symbol(), self.language())
    }

    /// The compilation dir of the function.
    pub fn compilation_dir(&self) -> &str {
        self.record.comp_dir.read_str(self.data).unwrap_or("")
    }

    /// An iterator over all lines in the function.
    pub fn lines(&self) -> Lines<'a> {
        Lines {
            lines: self.record.line_records,
            files: self.files,
            data: self.data,
            address: 0,
            index: 0,
        }
    }
}

struct LinesDebug<'a>(Lines<'a>);

impl fmt::Debug for LinesDebug<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut list = f.debug_list();
        for line in self.0.clone() {
            match line {
                Ok(line) => {
                    list.entry(&line);
                }
                Err(error) => {
                    return error.fmt(f);
                }
            }
        }
        list.finish()
    }
}

impl fmt::Debug for Function<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Function")
            .field("id", &self.id())
            .field("parent_id", &self.parent_id())
            .field("symbol", &self.symbol())
            .field("address", &self.address())
            .field("compilation_dir", &self.compilation_dir())
            .field("language", &self.language())
            .field("lines", &LinesDebug(self.lines()))
            .finish()
    }
}

#[derive(Clone)]
pub struct Lines<'a> {
    lines: format::Seg<format::LineRecord, u16>,
    files: format::Seg<format::FileRecord, u16>,
    data: &'a [u8],
    address: u64,
    index: u16,
}

impl<'a> Iterator for Lines<'a> {
    type Item = Result<Line<'a>, SymCacheError>;

    fn next(&mut self) -> Option<Self::Item> {
        let record = match self.lines.get(self.data, self.index) {
            Ok(Some(record)) => record,
            Ok(None) => return None,
            Err(error) => return Some(Err(error)),
        };

        self.address += u64::from(record.addr_off);
        self.index += 1;

        Some(Ok(Line {
            record,
            file: read_file_record(self.data, self.files, record.file_id).unwrap_or(None),
            address: self.address,
            data: self.data,
        }))
    }
}

pub struct Line<'a> {
    record: &'a format::LineRecord,
    file: Option<&'a format::FileRecord>,
    data: &'a [u8],
    address: u64,
}

impl<'a> Line<'a> {
    /// The address of the line.
    pub fn address(&self) -> u64 {
        self.address
    }

    /// The line number of the line.
    pub fn line(&self) -> u16 {
        self.record.line
    }

    /// The base_dir of the line.
    pub fn base_dir(&self) -> &str {
        match self.file {
            Some(ref record) => record.base_dir.read_str(self.data).unwrap_or(""),
            None => "",
        }
    }

    /// The filename of the line.
    pub fn filename(&self) -> &'a str {
        match self.file {
            Some(ref record) => record.filename.read_str(self.data).unwrap_or(""),
            None => "",
        }
    }
}

impl fmt::Debug for Line<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Line")
            .field("address", &self.address())
            .field("line", &self.line())
            .field("base_dir", &self.base_dir())
            .field("filename", &self.filename())
            .finish()
    }
}

/// Look up a single symbol.
fn read_symbol(
    data: &[u8],
    symbols: format::Seg<format::Seg<u8, u16>>,
    index: u32,
) -> Result<Option<&str>, SymCacheError> {
    if index == !0 {
        Ok(None)
    } else if let Some(symbol) = symbols.get(data, index)? {
        symbol.read_str(data).map(Some)
    } else {
        Ok(None)
    }
}

/// Look up a file record.
fn read_file_record(
    data: &[u8],
    files: format::Seg<format::FileRecord, u16>,
    index: u16,
) -> Result<Option<&format::FileRecord>, SymCacheError> {
    if index == !0 {
        Ok(None)
    } else {
        files.get(data, index)
    }
}
