use std::fmt;

use symbolic_common::{Arch, AsSelf, DebugId, Language, Name, NameMangling};

use crate::error::SymCacheError;
use crate::format;

/// A platform independent symbolication cache.
///
/// Use [`SymCacheWriter`] writer to create SymCaches, including the conversion from object files.
///
/// [`SymCacheWriter`]: struct.SymCacheWriter.html
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

    /// Returns whether this cache is up-to-date.
    pub fn is_latest(&self) -> bool {
        self.version() == format::SYMCACHE_VERSION
    }

    /// The architecture of the symbol file.
    pub fn arch(&self) -> Arch {
        Arch::from_u32(self.header.arch)
    }

    /// The debuig identifier of the cache file.
    pub fn debug_id(&self) -> DebugId {
        self.header.debug_id
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
        let mut current_id = match funcs.binary_search_by_key(&addr, format::FuncRecord::addr_start)
        {
            Ok(index) => index,
            Err(0) => return Ok(Lookup::empty(self)),
            Err(next) => next - 1,
        };

        // Seek forward to the deepest inlined function at the same address.
        while let Some(current_fn) = funcs.get(current_id + 1) {
            if current_fn.addr_start() != funcs[current_id].addr_start() {
                break;
            }
            current_id += 1;
        }

        // Find the function with the line record closest to the address. There are multiple ways
        // this lookup can go:
        //  a. The current function referred to by `current_id` contains the line record responsible
        //     for the address. However, line records only store the beginning and not the end of
        //     their range, so we need to check for case (d).
        //  b. The current function is top-level ends before the search address. This can happen due
        //     to incomplete debug information or padding sections in the code. There is no match
        //     for this case.
        //  c. Same as 2, but for inline functions. Even though the inlinee doesn't match, one of
        //     its ancestors might still cover with its range. They have to be checked until one
        //     covers the range. Still, case (d) can apply additionally.
        //  d. Even though a function covers the search address, it might be interleaved with
        //     another function that started earlier but contains a line record closer to the search
        //     address. See below for the lookup strategy.
        let mut closest = None;

        // Since functions with overlapping ranges can exist, we need to check multiple functions
        // until we hit a point where we believe no more functions can overlap. Theoretically, this
        // is the very start of the list. FOR PERFORMANCE REASONS, THIS IMPLEMENATION ONLY CHECKS
        // FOR OVERLAPS IN INLINE FUNCTIONS.
        let mut last_id = current_id;
        loop {
            let current_fn = &funcs[current_id];

            // If the current function covers the address, resolve the closest line record before
            // the search address. If it is closer than what we've seen before, this is a better
            // candidate, otherwise we can discard this function.
            if current_fn.addr_in_range(addr) {
                let current_addr = self
                    .run_to_line(current_fn, addr)?
                    // A lookup of `None` indicates that there was no line record at all, so just
                    // assume the function's start address as start of the line.
                    .map_or(current_fn.addr_start(), |(line_addr, _, _)| line_addr);

                if closest.map_or(true, |(_, _, a)| current_addr > a) {
                    closest = Some((current_id, current_fn, current_addr));
                }
            }

            // We are currently looking at an inline function. Since we're scanning linearly, ensure
            // that we're also including its parent. This might be from a completely different
            // inlining branch, so honor the existing `last_id` value as it might be lower.
            if let Some(parent_id) = current_fn.parent(current_id) {
                last_id = parent_id.min(last_id);
            }

            // We've checked the last function (inclusive), so bail out.
            if current_id == 0 || current_id == last_id {
                break;
            }

            // Continue with the immediate predecessor. This is not necessarily the inlining parent,
            // it might be a completely unrelated function from a different branch. Still, it might
            // cover the search address, so we cannot jump to the parent directly.
            current_id -= 1;
        }

        let (closest_id, closest_fn) = match closest {
            Some((closest_id, closest_fn, _)) => (closest_id, closest_fn),
            None => return Ok(Lookup::empty(self)),
        };

        Ok(Lookup {
            cache: self,
            funcs,
            current: Some((addr, closest_id, closest_fn)),
            inner: None,
        })
    }

    /// Resolves the raw list of `FuncRecords` from the funcs segment.
    fn function_records(&self) -> Result<&'a [format::FuncRecord], SymCacheError> {
        self.header.functions.read(self.data)
    }

    /// Locates the source line record for an instruction address within a function.
    ///
    /// This function runs through all line records of the given function and
    /// returns the line closest to the specified instruction. `addr` must be
    /// within the function range, otherwise the response is implementation
    /// defined. However, `addr` may point to any address within an instruction.
    ///
    /// Returns some tuple containing:
    ///  - `.0`: First instruction address of the source line
    ///  - `.1`: File id of the source file containing this line
    ///  - `.2`: Line number in the file
    ///
    /// Returns `None` if the function does not have line records.
    fn run_to_line(
        &self,
        fun: &format::FuncRecord,
        addr: u64,
    ) -> Result<Option<(u64, u16, u32)>, SymCacheError> {
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
        let mut running_addr = fun.addr_start();
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

        Ok(Some((line_addr, file_id, line)))
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
            if let Some((line_addr, file_id, line)) = self.run_to_line(fun, addr)? {
                // A missing file record indicates a bad symcache.
                let file_record = read_file_record(self.data, self.header.files, file_id)?
                    .ok_or_else(|| SymCacheError::BadCacheFile)?;

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
            debug_id: self.debug_id(),
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

impl<'slf, 'd: 'slf> AsSelf<'slf> for SymCache<'d> {
    type Ref = SymCache<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        self
    }
}

impl fmt::Debug for SymCache<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SymCache")
            .field("debug_id", &self.debug_id())
            .field("arch", &self.arch())
            .field("has_line_info", &self.has_line_info())
            .field("has_file_info", &self.has_file_info())
            .field("functions", &self.function_records().unwrap_or(&[]).len())
            .finish()
    }
}

/// An iterator over line matches for an address lookup.
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

    /// Collects all line matches into a collection.
    pub fn collect<B>(self) -> Result<B, SymCacheError>
    where
        B: std::iter::FromIterator<LineInfo<'a>>,
    {
        Iterator::collect(self)
    }
}

impl<'a, 'c> Iterator for Lookup<'a, 'c> {
    type Item = Result<LineInfo<'a>, SymCacheError>;

    fn next(&mut self) -> Option<Self::Item> {
        let (addr, id, fun) = self.current?;
        let line_result = self.cache.build_line_info(fun, addr, None);

        self.current = fun
            .parent(id)
            .map(|parent_id| (addr, parent_id, &self.funcs[parent_id]));

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
    debug_id: DebugId,
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
    pub fn debug_id(&self) -> DebugId {
        self.debug_id
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

    /// The joined path and file name relative to the compilation directory.
    pub fn path(&self) -> String {
        let joined = symbolic_common::join_path(self.base_dir, self.filename);
        symbolic_common::clean_path(&joined).into_owned()
    }

    /// The fully joined absolute path including the compilation directory.
    pub fn abs_path(&self) -> String {
        let joined_path = symbolic_common::join_path(self.base_dir, self.filename);
        let joined = symbolic_common::join_path(self.comp_dir, &joined_path);
        symbolic_common::clean_path(&joined).into_owned()
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
    pub fn function_name(&self) -> Name<'_> {
        Name::new(self.symbol(), NameMangling::Unknown, self.language())
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

/// A function in a `SymCache`.
///
/// This can be an actual function, an inlined function, or a public symbol.
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
    pub fn name(&self) -> Name<'_> {
        Name::new(self.symbol(), NameMangling::Unknown, self.language())
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

/// Helper for printing a human-readable debug representation of line records.
struct LinesDebug<'a>(Lines<'a>);

impl fmt::Debug for LinesDebug<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

/// An iterator over lines of a SymCache function.
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

/// A line covered by a [`Function`](struct.Function.html).
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
