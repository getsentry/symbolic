use std::str;
use std::fmt;
use std::mem;
use std::slice;
use std::cell::RefCell;
use std::io::{Write, Seek, SeekFrom};
use std::collections::BTreeSet;

use symbolic_common::{Error, ErrorKind, Result, ResultExt, Endianness};
use symbolic_debuginfo::{Object, DwarfSection};

use types::CacheFileHeader;

use gimli;
use fallible_iterator::FallibleIterator;

fn err(msg: &'static str) -> Error {
    Error::from(ErrorKind::BadDwarfData(msg))
}

pub fn write_sym_cache<W: Write + Seek>(w: W, obj: &Object) -> Result<()> {
    let mut writer = SymCacheWriter::new(w);
    // write the initial header into the file.  This positions the cursor after it.
    writer.write_header()?;
    // write object data.
    writer.write_object(obj)?;
    // write the updated header.
    writer.write_header()?;
    Ok(())
}

struct Function<'a> {
    pub depth: u8,
    pub name: &'a [u8],
    pub inlines: Vec<Function<'a>>,
    pub lines: Vec<Line<'a>>,
}

struct Line<'a> {
    pub addr: u64,
    pub comp_dir: &'a [u8],
    pub filename: &'a [u8],
    pub line: u32,
}

impl<'a> fmt::Debug for Line<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Line")
            .field("addr", &self.addr)
            .field("comp_dir", &String::from_utf8_lossy(self.comp_dir))
            .field("filename", &String::from_utf8_lossy(self.filename))
            .field("line", &self.line)
            .finish()
    }
}

impl<'a> fmt::Debug for Function<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Function")
            .field("name", &String::from_utf8_lossy(self.name))
            .field("inlines", &self.inlines)
            .field("lines", &self.lines)
            .finish()
    }
}

impl<'a> Function<'a> {
    pub fn append_line_if_changed(&mut self, line: Line<'a>) {
        if let Some(last_line) = self.lines.last() {
            if last_line.filename == line.filename &&
               last_line.comp_dir == line.comp_dir &&
               last_line.line == line.line {
                return;
            }
        }
        self.lines.push(line);
    }

    pub fn dedup_inlines(&mut self) {
        let mut inner_addrs = BTreeSet::new();
        for func in &self.inlines {
            for line in &func.lines {
                inner_addrs.insert(line.addr);
            }
        }

        if inner_addrs.is_empty() {
            return;
        }
        self.lines.retain(|item| !inner_addrs.contains(&item.addr));

        for func in self.inlines.iter_mut() {
            func.dedup_inlines();
        }
    }
}

pub struct SymCacheWriter<W: Write + Seek> {
    writer: RefCell<W>,
    header: CacheFileHeader,
}

#[derive(Debug)]
struct DwarfInfo<'input> {
    units: Vec<gimli::CompilationUnitHeader<gimli::EndianBuf<'input, Endianness>>>,
    debug_abbrev: gimli::DebugAbbrev<gimli::EndianBuf<'input, Endianness>>,
    debug_ranges: gimli::DebugRanges<gimli::EndianBuf<'input, Endianness>>,
    debug_line: gimli::DebugLine<gimli::EndianBuf<'input, Endianness>>,
    debug_str: gimli::DebugStr<gimli::EndianBuf<'input, Endianness>>,
}

impl<'input> DwarfInfo<'input> {
    fn from_object(obj: &'input Object) -> Result<DwarfInfo<'input>> {
        macro_rules! section {
            ($sect:ident, $mandatory:expr) => {{
                let sect = match obj.get_dwarf_section(DwarfSection::$sect) {
                    Some(sect) => sect.as_bytes(),
                    None => {
                        if $mandatory {
                            return Err(ErrorKind::MissingSection(
                                DwarfSection::$sect.get_elf_section()).into());
                        } else {
                            &[]
                        }
                    }
                };
                gimli::$sect::new(sect, obj.endianess())
            }}
        }

        Ok(DwarfInfo {
            units: section!(DebugInfo, true).units().collect()?,
            debug_abbrev: section!(DebugAbbrev, true),
            debug_line: section!(DebugLine, true),
            debug_ranges: section!(DebugRanges, false),
            debug_str: section!(DebugStr, false),
        })
    }
}

impl<W: Write + Seek> SymCacheWriter<W> {
    pub fn new(writer: W) -> SymCacheWriter<W> {
        SymCacheWriter {
            writer: RefCell::new(writer),
            header: Default::default(),
        }
    }

    fn with_file<T, F: FnOnce(&mut W) -> T>(&self, f: F) -> T {
        f(&mut *self.writer.borrow_mut() as &mut W)
    }

    fn write<T>(&self, x: &T) -> Result<usize> {
        unsafe {
            let bytes : *const u8 = mem::transmute(x);
            let size = mem::size_of_val(x);
            self.with_file(|writer| {
                writer.write_all(slice::from_raw_parts(bytes, size))
            })?;
            Ok(size)
        }
    }

    pub fn write_header(&self) -> Result<()> {
        self.with_file(|writer| {
            writer.seek(SeekFrom::Start(0))
        })?;
        self.write(&self.header)?;
        Ok(())
    }

    pub fn write_object(&mut self, obj: &Object) -> Result<()> {
        let info = DwarfInfo::from_object(obj)
            .chain_err(|| err("could not extract debug info from object file"))?;

        for header in &info.units {
            // attempt to parse a single unit from the given header.
            let unit_opt = Unit::parse(&info, header)
                .chain_err(|| err("encountered invalid compilation unit"))?;

            // skip units we don't care about
            let unit = match unit_opt {
                Some(unit) => unit,
                None => { continue; }
            };

            // dedup instructions from inline functions
            let functions = unit.functions()?;
            for mut func in functions {
                func.dedup_inlines();
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
struct Unit<'input> {
    info: &'input DwarfInfo<'input>,
    header: &'input gimli::CompilationUnitHeader<gimli::EndianBuf<'input, Endianness>>,
    abbrev: gimli::Abbreviations,
    base_address: u64,
    line_sequences: DwarfLineProgram<'input>,
    comp_dir: Option<gimli::EndianBuf<'input, Endianness>>,
    comp_name: Option<gimli::EndianBuf<'input, Endianness>>,
    language: Option<gimli::DwLang>,
}

impl<'input> Unit<'input> {
    fn parse(
        info: &'input DwarfInfo,
        header: &'input gimli::CompilationUnitHeader<gimli::EndianBuf<'input, Endianness>>,
    ) -> Result<Option<Unit<'input>>> {
        let abbrev = header
            .abbreviations(&info.debug_abbrev)
            .chain_err(|| err("compilation unit refers to non-existing abbreviations"))?;

        let base_address;
        let line_sequences;
        let comp_dir;
        let comp_name;
        let language;

        {
            let mut entries = header.entries(&abbrev);
            let (_, entry) = entries
                .next_dfs()
                .chain_err(|| err("compilation unit is broken"))?
                .ok_or_else(|| err("unit without compilation unit"))?;

            if entry.tag() != gimli::DW_TAG_compile_unit {
                return Err(err("missing compilation unit"));
            }

            base_address = match entry.attr_value(gimli::DW_AT_low_pc) {
                Ok(Some(gimli::AttributeValue::Addr(addr))) => addr,
                Err(e) => {
                    return Err(Error::from(e))
                        .chain_err(|| err("invalid low_pc attribute"))
                }
                _ => match entry.attr_value(gimli::DW_AT_entry_pc) {
                    Ok(Some(gimli::AttributeValue::Addr(addr))) => addr,
                    Err(e) => {
                        return Err(Error::from(e))
                            .chain_err(|| err("invalid entry_pc attribute"))
                    }
                    _ => 0,
                },
            };

            // Extract source file and line information about the compilation unit
            let line_offset = match entry.attr_value(gimli::DW_AT_stmt_list) {
                Ok(Some(gimli::AttributeValue::DebugLineRef(offset))) => offset,
                Err(e) => {
                    return Err(Error::from(e))
                        .chain_err(|| "invalid compilation unit statement list");
                }
                _ => {
                    return Ok(None);
                }
            };

            comp_dir = entry
                .attr(gimli::DW_AT_comp_dir)
                .map_err(|e| Error::from(e))
                .chain_err(|| err("invalid compilation unit directory"))?
                .and_then(|attr| attr.string_value(&info.debug_str));

            comp_name = entry
                .attr(gimli::DW_AT_name)
                .map_err(|e| Error::from(e))
                .chain_err(|| err("invalid compilation unit name"))?
                .and_then(|attr| attr.string_value(&info.debug_str));

            language = entry
                .attr(gimli::DW_AT_language)
                .map_err(|e| Error::from(e))?
                .and_then(|attr| match attr.value() {
                    gimli::AttributeValue::Language(lang) => Some(lang),
                    _ => None,
                });

            line_sequences = DwarfLineProgram::parse(
                info,
                line_offset,
                header.address_size(),
                comp_dir,
                comp_name,
            )?;
        }

        Ok(Some(Unit {
            info,
            header,
            abbrev,
            base_address,
            line_sequences,
            comp_dir,
            comp_name,
            language,
        }))
    }

    fn functions(&self) -> Result<Vec<Function<'input>>> {
        let mut depth = 0;
        let mut functions: Vec<Function> = vec![];
        let mut entries = self.header.entries(&self.abbrev);

        while let Some((movement, entry)) = entries
            .next_dfs()
            .chain_err(|| err("tree below compilation unit yielded invalid entry"))?
        {
            depth += movement;

            // skip anything that is not a function
            let inline = match entry.tag() {
                gimli::DW_TAG_subprogram => false,
                gimli::DW_TAG_inlined_subroutine => true,
                _ => { continue; }
            };

            let ranges = self.parse_ranges(entry)
                .chain_err(|| err("subroutine has invalid ranges"))?;
            if ranges.is_empty() {
                continue;
            }

            let mut func = Function {
                depth: depth as u8,
                name: self.resolve_function_name(entry)?.unwrap_or(b""),
                inlines: vec![],
                lines: vec![],
            };

            for range in &ranges {
                let rows = self.line_sequences.get_rows(range);
                for row in rows {
                    let comp_dir = self.comp_dir.as_ref().map(|x| x.buf()).unwrap_or(b"");
                    let filename = self.line_sequences.header
                        .file(row.file_index)
                        .map(|x| x.path_name().buf())
                        .unwrap_or(b"");

                    let new_line = Line {
                        addr: row.address,
                        filename: filename,
                        comp_dir: comp_dir,
                        line: row.line.unwrap_or(0) as u32,
                    };

                    func.append_line_if_changed(new_line);
                }
            }

            if inline {
                let mut node = functions.last_mut().expect("no root function");
                while { {&node}.inlines.last().map_or(false, |n| (n.depth as isize) < depth) } {
                    node = {node}.inlines.last_mut().unwrap();
                }
                node.inlines.push(func);
            } else {
                functions.push(func);
            }
        }

        Ok(functions)
    }

    fn parse_ranges(
        &self,
        entry: &gimli::DebuggingInformationEntry<gimli::EndianBuf<Endianness>>
    ) -> Result<Vec<gimli::Range>> {
        if let Some(range) = self.parse_noncontiguous_ranges(entry)? {
            Ok(range)
        } else if let Some(range) = Self::parse_contiguous_range(entry)? {
            Ok(vec![range])
        } else {
            Ok(vec![])
        }
    }

    fn parse_noncontiguous_ranges(
        &self,
        entry: &gimli::DebuggingInformationEntry<gimli::EndianBuf<Endianness>>
    ) -> Result<Option<Vec<gimli::Range>>> {
        let offset = match entry.attr_value(gimli::DW_AT_ranges) {
            Ok(Some(gimli::AttributeValue::DebugRangesRef(offset))) => offset,
            Err(e) => {
                return Err(Error::from(e)).chain_err(|| err("invalid ranges attribute"));
            }
            _ => return Ok(None),
        };

        let ranges = self.info.debug_ranges
            .ranges(offset, self.header.address_size(), self.base_address)
            .chain_err(|| err("range offsets are not valid"))?;
        let ranges = ranges.collect().chain_err(|| err("range could not be parsed"))?;
        Ok(Some(ranges))
    }

    fn parse_contiguous_range(
        entry: &gimli::DebuggingInformationEntry<gimli::EndianBuf<Endianness>>,
    ) -> Result<Option<gimli::Range>> {
        let low_pc = match entry.attr_value(gimli::DW_AT_low_pc) {
            Ok(Some(gimli::AttributeValue::Addr(addr))) => addr,
            Err(e) => {
                return Err(Error::from(e))
                    .chain_err(|| err("invalid low_pc attribute"))
            }
            _ => return Ok(None),
        };

        let high_pc = match entry.attr_value(gimli::DW_AT_high_pc) {
            Ok(Some(gimli::AttributeValue::Addr(addr))) => addr,
            Ok(Some(gimli::AttributeValue::Udata(size))) => low_pc.wrapping_add(size),
            Err(e) => {
                return Err(Error::from(e)).chain_err(|| err("invalid high_pc attribute"))
            }
            _ => return Ok(None),
        };

        if low_pc == 0 {
            // https://sourceware.org/git/gitweb.cgi?p=binutils-gdb.git;a=blob;f=gdb/dwarf2read.c;h=ed10e03812f381ccdb5c51e1c689df8d61ab87f6;hb=HEAD#l16000
            // TODO: *technically* there could be a relocatable section placed at VA 0
            return Ok(None);
        }

        if low_pc == high_pc {
            // https://sourceware.org/ml/gdb-patches/2011-03/msg00739.html
            return Ok(None);
        }

        if low_pc > high_pc {
            return Err(err("invalid due to inverted range"));
        }

        Ok(Some(gimli::Range {
            begin: low_pc,
            end: high_pc,
        }))
    }

    fn resolve_reference<'a>(
        &'a self,
        base_entry: &gimli::DebuggingInformationEntry<'a, 'a, gimli::EndianBuf<'input, Endianness>>,
        attr: gimli::DwAt,
    ) -> Result<Option<gimli::DebuggingInformationEntry<'a, 'a, gimli::EndianBuf<'input, Endianness>>>> {
        let (header, offset) = match base_entry.attr_value(attr)? {
            Some(gimli::AttributeValue::UnitRef(offset)) => {
                (self.header, offset)
            },
            Some(gimli::AttributeValue::DebugInfoRef(offset)) => {
                // TODO(ja): Implement
                if let Some(unit_offset) = offset.to_unit_offset(self.header) {
                    (self.header, unit_offset)
                } else {
                    // is this happening in real life?  This would require us to
                    // either parse other stuff again or cache all units.
                    return Ok(None);
                }
            },
            None => {
                return Ok(None);
            },
            _ => {
                // TODO: there is probably more that can come back here
                return Ok(None);
            },
        };

        // TODO(ja): This doesn't work here, we need the unit's abbrev...
        let mut entries = header.entries_at_offset(&self.abbrev, offset)?;
        let (_, entry) = entries
            .next_dfs()?
            .ok_or_else(|| {
                err("invalid debug symbols: dangling entry offset")
            })?;
        Ok(Some(entry.clone()))
    }

    fn resolve_function_name<'a, 'b>(
        &self,
        entry: &gimli::DebuggingInformationEntry<'a, 'b, gimli::EndianBuf<'input, Endianness>>,
    ) -> Result<Option<&'input [u8]>> {
        // For naming, we prefer the linked name, if available
        if let Some(name) = entry
            .attr(gimli::DW_AT_linkage_name)
            .map_err(|e| Error::from(e))
            .chain_err(|| err("invalid subprogram linkage name"))?
            .and_then(|attr| attr.string_value(&self.info.debug_str))
        {
            return Ok(Some(name.buf()));
        }
        if let Some(name) = entry
            .attr(gimli::DW_AT_MIPS_linkage_name)
            .map_err(|e| Error::from(e))
            .chain_err(|| err("invalid subprogram linkage name"))?
            .and_then(|attr| attr.string_value(&self.info.debug_str))
        {
            return Ok(Some(name.buf()));
        }

        // Linked name is not available, so fall back to just plain old name, if that's available.
        if let Some(name) = entry
            .attr(gimli::DW_AT_name)
            .map_err(|e| Error::from(e))
            .chain_err(|| err("invalid subprogram name"))?
            .and_then(|attr| attr.string_value(&self.info.debug_str))
        {
            return Ok(Some(name.buf()));
        }

        // If we don't have the link name, check if this function refers to another
        if let Some(abstract_origin) =
            self.resolve_reference(entry, gimli::DW_AT_abstract_origin)
                .chain_err(|| err("invalid subprogram abstract origin"))?
        {
            let name = self.resolve_function_name(&abstract_origin)
                .chain_err(|| err("abstract origin does not resolve to a name"))?;
            return Ok(name);
        }
        if let Some(specification) =
            self.resolve_reference(entry, gimli::DW_AT_specification)
                .chain_err(|| err("invalid subprogram specification"))?
        {
            let name = self.resolve_function_name(&specification)
                .chain_err(|| err("specification does not resolve to a name"))?;
            return Ok(name);
        }

        Ok(None)
    }
}

#[derive(Debug)]
struct DwarfLineProgram<'input> {
    sequences: Vec<DwarfSeq>,
    header: gimli::LineNumberProgramHeader<gimli::EndianBuf<'input, Endianness>>,
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
    fn parse(
        info: &'input DwarfInfo,
        line_offset: gimli::DebugLineOffset,
        address_size: u8,
        comp_dir: Option<gimli::EndianBuf<'input, Endianness>>,
        comp_name: Option<gimli::EndianBuf<'input, Endianness>>,
    ) -> Result<Self> {
        let program = info.debug_line
            .program(line_offset, address_size, comp_dir, comp_name)?;

        let mut sequences = vec![];
        let mut sequence_rows: Vec<DwarfRow> = vec![];
        let mut prev_address = 0;
        let mut program_rows = program.rows();

        // XXX: do we need a clone here?  Maybe we can do better
        let header = program_rows.header().clone();
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

        // make sure this is sorted
        sequences.sort_by(|a, b| a.low_address.cmp(&b.low_address));

        Ok(DwarfLineProgram {
            sequences: sequences,
            header: header,
        })
    }

    pub fn get_rows(&self, rng: &gimli::Range) -> &[DwarfRow] {
        for seq in &self.sequences {
            if seq.high_address < rng.begin || seq.low_address > rng.end {
                continue;
            }
            let mut start = !0;
            let mut end = !0;
            for (idx, ref row) in seq.rows.iter().enumerate() {
                if row.address >= rng.begin && start == !0 {
                    start = idx;
                } else if row.address > rng.end - 1 {
                    end = idx;
                    break;
                }
            }
            if start == !0 {
                continue;
            }
            if end == !0 {
                end = seq.rows.len();
            }
            return &seq.rows[start..end]
        }
        &[]
    }
}
