use std::cell::RefCell;
use std::fmt;
use std::cmp;
use std::mem;
use std::sync::Arc;

use symbolic_common::{Endianness, Error, ErrorKind, Language, Result, ResultExt};
use symbolic_debuginfo::{DwarfData, DwarfSection, Object, Symbols};

use fallible_iterator::FallibleIterator;
use lru_cache::LruCache;
use fnv::FnvBuildHasher;
use dmsort;
use gimli::{self, Abbreviations, AttributeValue, CompilationUnitHeader, DebugAbbrev,
            DebugAbbrevOffset, DebugInfoOffset, DebugLine, DebugLineOffset, DebugRanges, DebugStr,
            DebuggingInformationEntry, DwLang, EndianBuf, IncompleteLineNumberProgram, Range,
            StateMachine, UnitOffset};

type Buf<'input> = EndianBuf<'input, Endianness>;
type Die<'abbrev, 'unit, 'input> = DebuggingInformationEntry<'abbrev, 'unit, Buf<'input>>;

fn err(msg: &'static str) -> Error {
    Error::from(ErrorKind::BadDwarfData(msg))
}

#[derive(Debug)]
pub struct DwarfInfo<'input> {
    pub units: Vec<CompilationUnitHeader<Buf<'input>>>,
    pub debug_abbrev: DebugAbbrev<Buf<'input>>,
    pub debug_ranges: DebugRanges<Buf<'input>>,
    pub debug_line: DebugLine<Buf<'input>>,
    pub debug_str: DebugStr<Buf<'input>>,
    pub vmaddr: u64,
    abbrev_cache: RefCell<LruCache<DebugAbbrevOffset<usize>, Arc<Abbreviations>, FnvBuildHasher>>,
}

impl<'input> DwarfInfo<'input> {
    pub fn from_object(obj: &'input Object) -> Result<DwarfInfo<'input>> {
        macro_rules! section {
            ($sect:ident, $mandatory:expr) => {{
                let sect = match obj.get_dwarf_section(DwarfSection::$sect) {
                    Some(sect) => sect.as_bytes(),
                    None => {
                        if $mandatory {
                            return Err(ErrorKind::MissingSection(
                                DwarfSection::$sect.name()).into());
                        }
                        &[]
                    }
                };
                gimli::$sect::new(sect, obj.endianness())
            }}
        }

        Ok(DwarfInfo {
            units: section!(DebugInfo, true).units().collect()?,
            debug_abbrev: section!(DebugAbbrev, true),
            debug_line: section!(DebugLine, true),
            debug_ranges: section!(DebugRanges, false),
            debug_str: section!(DebugStr, false),
            vmaddr: obj.vmaddr()?,
            abbrev_cache: RefCell::new(LruCache::with_hasher(30, Default::default())),
        })
    }

    #[inline(always)]
    pub fn get_unit_header(&self, index: usize) -> Result<&CompilationUnitHeader<Buf<'input>>> {
        self.units
            .get(index)
            .ok_or_else(|| err("non existing unit"))
    }

    pub fn get_abbrev(
        &self,
        header: &CompilationUnitHeader<Buf<'input>>,
    ) -> Result<Arc<Abbreviations>> {
        let offset = header.debug_abbrev_offset();
        let mut cache = self.abbrev_cache.borrow_mut();
        if let Some(abbrev) = cache.get_mut(&offset) {
            return Ok(abbrev.clone());
        }

        let abbrev = header
            .abbreviations(&self.debug_abbrev)
            .chain_err(|| err("compilation unit refers to non-existing abbreviations"))?;

        cache.insert(offset, Arc::new(abbrev));
        Ok(cache.get_mut(&offset).unwrap().clone())
    }

    fn find_unit_offset(
        &self,
        offset: DebugInfoOffset<usize>,
    ) -> Result<(usize, UnitOffset<usize>)> {
        let idx = match self.units.binary_search_by_key(&offset.0, |x| x.offset().0) {
            Ok(idx) => idx,
            Err(0) => return Err(err("couln't find unit for ref address")),
            Err(next_idx) => next_idx - 1,
        };
        let header = &self.units[idx];
        if let Some(unit_offset) = offset.to_unit_offset(header) {
            return Ok((idx, unit_offset));
        }
        Err(err("unit offset not within unit"))
    }
}

pub struct Line<'a> {
    pub addr: u64,
    pub original_file_id: u64,
    pub filename: &'a [u8],
    pub base_dir: &'a [u8],
    pub line: u16,
}

impl<'a> fmt::Debug for Line<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Line")
            .field("addr", &self.addr)
            .field("base_dir", &String::from_utf8_lossy(self.base_dir))
            .field("original_file_id", &self.original_file_id)
            .field("filename", &String::from_utf8_lossy(self.filename))
            .field("line", &self.line)
            .finish()
    }
}

pub struct Function<'a> {
    pub depth: u16,
    pub addr: u64,
    pub len: u32,
    pub name: &'a [u8],
    pub inlines: Vec<Function<'a>>,
    pub lines: Vec<Line<'a>>,
    pub comp_dir: &'a [u8],
    pub lang: Language,
}

impl<'a> Function<'a> {
    pub fn append_line_if_changed(&mut self, line: Line<'a>) {
        if let Some(last_line) = self.lines.last() {
            if last_line.original_file_id == line.original_file_id && last_line.line == line.line {
                return;
            }
        }
        self.lines.push(line);
    }

    pub fn get_addr(&self) -> u64 {
        self.addr
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
            && (self.inlines.is_empty() || self.inlines.iter().all(|x| x.is_empty()))
    }
}

impl<'a> fmt::Debug for Function<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Function")
            .field("name", &String::from_utf8_lossy(self.name))
            .field("addr", &self.addr)
            .field("len", &self.len)
            .field("depth", &self.depth)
            .field("inlines", &self.inlines)
            .field("comp_dir", &String::from_utf8_lossy(self.comp_dir))
            .field("lang", &self.lang)
            .field("lines", &self.lines)
            .finish()
    }
}

#[derive(Debug)]
pub struct Unit<'input> {
    index: usize,
    base_address: u64,
    comp_dir: Option<Buf<'input>>,
    comp_name: Option<Buf<'input>>,
    language: Option<DwLang>,
    line_offset: DebugLineOffset,
}

impl<'input> Unit<'input> {
    pub fn parse(info: &DwarfInfo<'input>, index: usize) -> Result<Option<Unit<'input>>> {
        let header = info.get_unit_header(index)?;

        // Access the compilation unit, which must be the top level DIE
        let abbrev = info.get_abbrev(header)?;
        let mut entries = header.entries(&*abbrev);
        let entry = match entries
            .next_dfs()
            .chain_err(|| err("compilation unit is broken"))?
        {
            Some((_, entry)) => entry,
            None => return Ok(None),
        };

        if entry.tag() != gimli::DW_TAG_compile_unit {
            return Err(err("missing compilation unit"));
        }

        let base_address = match entry.attr_value(gimli::DW_AT_low_pc) {
            Ok(Some(AttributeValue::Addr(addr))) => addr,
            Err(e) => {
                return Err(e).chain_err(|| err("invalid low_pc attribute"));
            }
            _ => match entry.attr_value(gimli::DW_AT_entry_pc) {
                Ok(Some(AttributeValue::Addr(addr))) => addr,
                Err(e) => {
                    return Err(e).chain_err(|| err("invalid entry_pc attribute"));
                }
                _ => 0,
            },
        };

        let comp_dir = entry
            .attr(gimli::DW_AT_comp_dir)
            .chain_err(|| err("invalid compilation unit directory"))?
            .and_then(|attr| attr.string_value(&info.debug_str));

        let comp_name = entry
            .attr(gimli::DW_AT_name)
            .chain_err(|| err("invalid compilation unit name"))?
            .and_then(|attr| attr.string_value(&info.debug_str));

        let language = entry
            .attr(gimli::DW_AT_language)
            .chain_err(|| err("invalid language"))?
            .and_then(|attr| match attr.value() {
                AttributeValue::Language(lang) => Some(lang),
                _ => None,
            });

        let line_offset = match entry.attr_value(gimli::DW_AT_stmt_list) {
            Ok(Some(AttributeValue::DebugLineRef(offset))) => offset,
            Err(e) => {
                return Err(e).chain_err(|| "invalid compilation unit statement list");
            }
            _ => {
                return Ok(None);
            }
        };

        Ok(Some(Unit {
            index,
            base_address,
            comp_dir,
            comp_name,
            language,
            line_offset,
        }))
    }

    pub fn get_functions(
        &self,
        info: &DwarfInfo<'input>,
        range_buf: &mut Vec<Range>,
        symbols: Option<&'input Symbols<'input>>,
        funcs: &mut Vec<Function<'input>>,
    ) -> Result<()> {
        let mut depth = 0;
        let header = info.get_unit_header(self.index)?;
        let abbrev = info.get_abbrev(header)?;
        let mut entries = header.entries(&*abbrev);

        let line_program = DwarfLineProgram::parse(
            info,
            self.line_offset,
            header.address_size(),
            self.comp_dir,
            self.comp_name,
        )?;

        while let Some((movement, entry)) = entries
            .next_dfs()
            .chain_err(|| err("tree below compilation unit yielded invalid entry"))?
        {
            depth += movement;

            // skip anything that is not a function
            let inline = match entry.tag() {
                gimli::DW_TAG_subprogram => false,
                gimli::DW_TAG_inlined_subroutine => true,
                _ => continue,
            };

            let ranges = self.parse_ranges(info, entry, range_buf)
                .chain_err(|| err("subroutine has invalid ranges"))?;
            if ranges.is_empty() {
                continue;
            }

            // try to find the symbol in the symbol table first if we are not an
            // inlined function.
            //
            // XXX: maybe we should actually parse the ranges in the resolve
            // function and always look at the symbol table based on the start of
            // the Die range.
            let func_name = if_chain! {
                if !inline;
                if let Some(symbols) = symbols;
                if let Some(symbol) = symbols.lookup(ranges[0].begin)?;
                if symbol.addr() + symbol.len().unwrap_or(!0) <= ranges[ranges.len() - 1].end;
                then {
                    Some(symbol.name())
                } else {
                    // fall back to dwarf info
                    self.resolve_function_name(info, entry)?
                }
            };

            let mut func = Function {
                depth: depth as u16,
                addr: ranges[0].begin - info.vmaddr,
                len: (ranges[ranges.len() - 1].end - ranges[0].begin) as u32,
                name: func_name.unwrap_or(b""),
                inlines: vec![],
                lines: vec![],
                comp_dir: self.comp_dir.map(|x| x.buf()).unwrap_or(b""),
                lang: self.language
                    .map(|lang| Language::from_dwarf_lang(lang))
                    .unwrap_or(Language::Unknown),
            };

            for range in ranges {
                let rows = line_program.get_rows(range);
                for row in rows {
                    let (base_dir, filename) = line_program.get_filename(row.file_index)?;

                    let new_line = Line {
                        addr: row.address - info.vmaddr,
                        original_file_id: row.file_index as u64,
                        filename: filename,
                        base_dir: base_dir,
                        line: cmp::min(row.line.unwrap_or(0), 0xffff) as u16,
                    };

                    func.append_line_if_changed(new_line);
                }
            }

            if inline {
                if funcs.is_empty() {
                    return Err(err("no root function"));
                }
                let mut node = funcs.last_mut().unwrap();
                while { &node }.inlines.last().map_or(false, |n| (n.depth as isize) < depth) {
                    node = { node }.inlines.last_mut().unwrap();
                }
                node.inlines.push(func);
            } else {
                funcs.push(func);
            }
        }

        // we definitely have to sort this here.  Functions unfortunately do not
        // appear sorted in dwarf files.
        dmsort::sort_by_key(funcs, |x| x.addr);

        Ok(())
    }

    fn parse_ranges<'a>(
        &self,
        info: &DwarfInfo<'input>,
        entry: &Die,
        buf: &'a mut Vec<Range>,
    ) -> Result<&'a [Range]> {
        let mut low_pc = None;
        let mut high_pc = None;
        let mut high_pc_rel = None;

        buf.clear();

        macro_rules! set_pc {
            ($var:ident, $val:expr) => {{
                $var = Some($val);
                if low_pc.is_some() && (high_pc.is_some() || high_pc_rel.is_some()) {
                    break;
                }
            }}
        }

        let mut fiter = entry.attrs();
        while let Some(attr) = fiter.next()? {
            match attr.name() {
                gimli::DW_AT_ranges => {
                    match attr.value() {
                        AttributeValue::DebugRangesRef(offset) => {
                            let header = info.get_unit_header(self.index)?;
                            let mut fiter = info.debug_ranges
                                .ranges(offset, header.address_size(), self.base_address)
                                .chain_err(|| err("range offsets are not valid"))?;

                            while let Some(item) = fiter.next()? {
                                buf.push(item);
                            }
                            return Ok(&buf[..]);
                        }
                        // XXX: error?
                        _ => continue,
                    }
                }
                gimli::DW_AT_low_pc => match attr.value() {
                    AttributeValue::Addr(addr) => set_pc!(low_pc, addr),
                    _ => return Ok(&[]),
                },
                gimli::DW_AT_high_pc => match attr.value() {
                    AttributeValue::Addr(addr) => set_pc!(high_pc, addr),
                    AttributeValue::Udata(size) => set_pc!(high_pc_rel, size),
                    _ => return Ok(&[]),
                },
                _ => continue,
            }
        }

        // to go by the logic in dwarf2read a low_pc of 0 can indicate an
        // eliminated duplicate when the GNU linker is used.
        // TODO: *technically* there could be a relocatable section placed at VA 0
        let low_pc = match low_pc {
            Some(low_pc) if low_pc != 0 => low_pc,
            _ => return Ok(&[]),
        };

        let high_pc = match (high_pc, high_pc_rel) {
            (Some(high_pc), _) => high_pc,
            (_, Some(high_pc_rel)) => low_pc.wrapping_add(high_pc_rel),
            _ => return Ok(&[]),
        };

        if low_pc == high_pc {
            // most likely low_pc == high_pc means the DIE should be ignored.
            // https://sourceware.org/ml/gdb-patches/2011-03/msg00739.html
            return Ok(&[]);
        }

        if low_pc > high_pc {
            // XXX: consider swallowing errors?
            return Err(err("invalid due to inverted range"));
        }

        buf.push(Range {
            begin: low_pc,
            end: high_pc,
        });
        Ok(&buf[..])
    }

    /// Resolves an entry and if found invokes a function to transform it.
    ///
    /// As this might resolve into cached information the data borrowed from
    /// abbrev can only be temporarily accessed in the callback.
    fn resolve_reference<'info, T, F>(
        &self,
        info: &'info DwarfInfo<'input>,
        attr_value: AttributeValue<Buf<'input>>,
        f: F,
    ) -> Result<Option<T>>
    where
        for<'abbrev> F: FnOnce(&Die<'abbrev, 'info, 'input>) -> Result<Option<T>>,
    {
        let (index, offset) = match attr_value {
            AttributeValue::UnitRef(offset) => (self.index, offset),
            AttributeValue::DebugInfoRef(offset) => {
                let (index, unit_offset) = info.find_unit_offset(offset)?;
                (index, unit_offset)
            }
            // TODO: there is probably more that can come back here
            _ => return Ok(None),
        };

        let header = info.get_unit_header(index)?;
        let abbrev = info.get_abbrev(header)?;
        let mut entries = header.entries_at_offset(&*abbrev, offset)?;
        entries.next_entry()?;
        if let Some(entry) = entries.current() {
            f(entry)
        } else {
            Ok(None)
        }
    }

    /// Resolves the function name of a debug entry.
    fn resolve_function_name<'abbrev, 'unit>(
        &self,
        info: &DwarfInfo<'input>,
        entry: &Die<'abbrev, 'unit, 'input>,
    ) -> Result<Option<&'input [u8]>> {
        let mut fiter = entry.attrs();
        let mut fallback_name = None;
        let mut reference_target = None;

        while let Some(attr) = fiter.next()? {
            match attr.name() {
                // prioritize these.  If we get them, take them.
                gimli::DW_AT_linkage_name | gimli::DW_AT_MIPS_linkage_name => {
                    return Ok(attr.string_value(&info.debug_str).map(|x| x.buf()));
                }
                gimli::DW_AT_name => {
                    fallback_name = Some(attr);
                }
                gimli::DW_AT_abstract_origin | gimli::DW_AT_specification => {
                    reference_target = Some(attr);
                }
                _ => {}
            }
        }

        if let Some(attr) = fallback_name {
            return Ok(attr.string_value(&info.debug_str).map(|x| x.buf()));
        }

        if let Some(attr) = reference_target {
            if let Some(name) = self.resolve_reference(info, attr.value(), |ref_entry| {
                self.resolve_function_name(info, ref_entry)
                    .chain_err(|| err("reference does not resolve to a name"))
            })? {
                return Ok(Some(name));
            }
        }

        Ok(None)
    }
}

#[derive(Debug)]
struct DwarfLineProgram<'input> {
    sequences: Vec<DwarfSeq>,
    program_rows: StateMachine<Buf<'input>, IncompleteLineNumberProgram<Buf<'input>>>,
}

#[derive(Debug)]
struct DwarfSeq {
    low_address: u64,
    high_address: u64,
    rows: Vec<DwarfRow>,
}

#[derive(Debug, PartialEq, Eq)]
struct DwarfRow {
    address: u64,
    file_index: u64,
    line: Option<u64>,
}

impl<'input> DwarfLineProgram<'input> {
    fn parse<'info>(
        info: &'info DwarfInfo<'input>,
        line_offset: DebugLineOffset,
        address_size: u8,
        comp_dir: Option<Buf<'input>>,
        comp_name: Option<Buf<'input>>,
    ) -> Result<Self> {
        let program = info.debug_line
            .program(line_offset, address_size, comp_dir, comp_name)?;

        let mut sequences = vec![];
        let mut sequence_rows: Vec<DwarfRow> = vec![];
        let mut prev_address = 0;
        let mut program_rows = program.rows();

        while let Ok(Some((_, &program_row))) = program_rows.next_row() {
            let address = program_row.address();
            if program_row.end_sequence() {
                if !sequence_rows.is_empty() {
                    let low_address = sequence_rows[0].address;
                    let high_address = if address < prev_address {
                        prev_address + 1
                    } else {
                        address
                    };
                    let mut rows = vec![];
                    mem::swap(&mut rows, &mut sequence_rows);
                    sequences.push(DwarfSeq {
                        low_address,
                        high_address,
                        rows,
                    });
                }
                prev_address = 0;
            } else if address < prev_address {
                // The standard says:
                // "Within a sequence, addresses and operation pointers may only increase."
                // So this row is invalid, we can ignore it.
                //
                // If we wanted to handle this, we could start a new sequence
                // here, but let's wait until that is needed.
            } else {
                let file_index = program_row.file_index();
                let line = program_row.line();
                let mut duplicate = false;
                if let Some(last_row) = sequence_rows.last_mut() {
                    if last_row.address == address {
                        last_row.file_index = file_index;
                        last_row.line = line;
                        duplicate = true;
                    }
                }
                if !duplicate {
                    sequence_rows.push(DwarfRow {
                        address,
                        file_index,
                        line,
                    });
                }
                prev_address = address;
            }
        }
        if !sequence_rows.is_empty() {
            // A sequence without an end_sequence row.
            // Let's assume the last row covered 1 byte.
            let low_address = sequence_rows[0].address;
            let high_address = prev_address + 1;
            sequences.push(DwarfSeq {
                low_address,
                high_address,
                rows: sequence_rows,
            });
        }

        // we might have to sort this here :(
        dmsort::sort_by_key(&mut sequences, |x| x.low_address);

        Ok(DwarfLineProgram {
            sequences: sequences,
            program_rows: program_rows,
        })
    }

    pub fn get_filename(&self, idx: u64) -> Result<(&'input [u8], &'input [u8])> {
        let header = self.program_rows.header();
        let file = header
            .file(idx)
            .ok_or_else(|| ErrorKind::BadDwarfData("invalid file reference"))?;
        Ok((
            file.directory(header).map(|x| x.buf()).unwrap_or(b""),
            file.path_name().buf(),
        ))
    }

    pub fn get_rows(&self, rng: &Range) -> &[DwarfRow] {
        for seq in &self.sequences {
            if seq.high_address < rng.begin || seq.low_address > rng.end {
                continue;
            }

            let start = match seq.rows.binary_search_by_key(&rng.begin, |x| x.address) {
                Ok(idx) => idx,
                Err(0) => continue,
                Err(next_idx) => next_idx - 1,
            };

            let len = seq.rows[start..]
                .binary_search_by_key(&rng.end, |x| x.address)
                .unwrap_or_else(|e| e);
            return &seq.rows[start..start + len];
        }
        &[]
    }
}
