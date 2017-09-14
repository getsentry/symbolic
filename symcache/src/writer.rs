use std::str;
use std::mem;
use std::io::Write;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::marker::PhantomData;

use symbolic_common::{Error, ErrorKind, Result, ResultExt, Endianness};
use symbolic_debuginfo::{Object, DwarfSection};

use gimli;
use fallible_iterator::FallibleIterator;

fn err(msg: &'static str) -> Error {
    Error::from(ErrorKind::BadDwarfData(msg))
}

struct StringRegistry {
    files: HashMap<Vec<u8>, u16>,
    symbols: HashMap<Vec<u8>, u32>,
}

impl StringRegistry {
    pub fn new() -> StringRegistry {
        StringRegistry {
            files: HashMap::new(),
            symbols: HashMap::new(),
        }
    }

    pub fn get_symbol_id(&mut self, sym: &[u8]) -> Result<u32> {
        if let Some(&idx) = self.symbols.get(sym) {
            Ok(idx)
        } else {
            let idx = self.symbols.len() as u32;
            if idx == !0 {
                Err(ErrorKind::Internal("Too many symbols").into())
            } else {
                self.symbols.insert(sym.to_vec(), idx);
                Ok(idx)
            }
        }
    }

    pub fn get_file_id(&mut self, file: &[u8]) -> Result<u16> {
        if let Some(&idx) = self.files.get(file) {
            Ok(idx)
        } else {
            let idx = self.files.len() as u16;
            if idx == !0 {
                Err(ErrorKind::Internal("Too many files").into())
            } else {
                self.files.insert(file.to_vec(), idx);
                Ok(idx)
            }
        }
    }
}

pub struct SymCacheWriter<W: Write> {
    writer: W,
}

impl<W: Write> SymCacheWriter<W> {
    pub fn new(writer: W) -> SymCacheWriter<W> {
        SymCacheWriter {
            writer: writer,
        }
    }

    pub fn write_object(&mut self, obj: &Object) -> Result<()> {
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

        let mut string_registry = StringRegistry::new();
        let debug_info = section!(DebugInfo, true);
        let debug_abbrev = section!(DebugAbbrev, true);
        let debug_line = section!(DebugLine, true);
        let debug_ranges = section!(DebugRanges, false);
        let debug_str = section!(DebugStr, false);

        let mut headers = debug_info.units();

        while let Some(header) = headers.next()
                .chain_err(|| err("couldn't get DIE header"))? {
            let unit_opt = Unit::try_parse(
                &mut string_registry,
                &debug_abbrev,
                &debug_ranges,
                &debug_line,
                &debug_str,
                &header,
            ).chain_err(|| err("encountered invalid compilation unit"))?;
            let unit = match unit_opt {
                Some(unit) => unit,
                None => { continue; }
            };
            //println!("{:#?}", unit);
        }

        Ok(())
    }
}


#[derive(Debug)]
struct Unit<'input> {
    _x: PhantomData<&'input ()>,
}

impl<'input> Unit<'input> {
    fn try_parse(
        string_registry: &mut StringRegistry,
        debug_abbrev: &gimli::DebugAbbrev<gimli::EndianBuf<Endianness>>,
        debug_ranges: &gimli::DebugRanges<gimli::EndianBuf<Endianness>>,
        debug_line: &gimli::DebugLine<gimli::EndianBuf<'input, Endianness>>,
        debug_str: &gimli::DebugStr<gimli::EndianBuf<'input, Endianness>>,
        header: &gimli::CompilationUnitHeader<gimli::EndianBuf<'input, Endianness>>,
    ) -> Result<Option<Unit<'input>>> {
        let abbrev = header
            .abbreviations(debug_abbrev)
            .chain_err(|| err("compilation unit refers to non-existing abbreviations"))?;
        let mut entries = header.entries(&abbrev);
        let base_address;
        let lines;
        let comp_dir;
        let comp_name;
        let language;
        {

            // Scoped so that we can continue using entries for the loop below
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
                .and_then(|attr| attr.string_value(debug_str));
            comp_name = entry
                .attr(gimli::DW_AT_name)
                .map_err(|e| Error::from(e))
                .chain_err(|| err("invalid compilation unit name"))?
                .and_then(|attr| attr.string_value(debug_str));
            language = entry
                .attr(gimli::DW_AT_language)
                .map_err(|e| Error::from(e))?
                .and_then(|attr| match attr.value() {
                    gimli::AttributeValue::Language(lang) => Some(lang),
                    _ => None,
                });

            lines = Lines::new(
                debug_line,
                line_offset,
                header.address_size(),
                comp_dir,
                comp_name,
            )?;
        }

        while let Some((_, entry)) = entries
            .next_dfs()
            .chain_err(|| err("tree below compilation unit yielded invalid entry"))?
        {
            // skip anything that is not a function
            let inline = match entry.tag() {
                gimli::DW_TAG_subprogram => false,
                gimli::DW_TAG_inlined_subroutine => true,
                _ => { continue; }
            };

            let ranges = Self::get_ranges(entry, debug_ranges, header.address_size(), base_address)
                .chain_err(|| err("subroutine has invalid ranges"))?;
            if ranges.is_empty() {
                continue;
            }

            let func_name = Self::resolve_function_name(entry, header, debug_str, &abbrev)?
                .map(|x| x.buf())
                .unwrap_or(b"");
            println!("{}", str::from_utf8(func_name).unwrap());

            for range in &ranges {
                let rows = lines.get_rows(range);
                for row in rows {
                    let comp_dir = comp_dir.as_ref().map(|x| x.buf()).unwrap_or(b"");
                    let file_record = lines.header
                        .file(row.file_index)
                        .map(|x| x.path_name().buf())
                        .unwrap_or(b"");
                    let line = row.line.unwrap_or(0);
                    println!("  at {}/{}:{}", str::from_utf8(comp_dir).unwrap(), str::from_utf8(file_record).unwrap(), line);
                }
            }
        }

        Ok(Some(Unit {
            _x: PhantomData,
        }))
    }

    fn get_ranges(
        entry: &gimli::DebuggingInformationEntry<gimli::EndianBuf<Endianness>>,
        debug_ranges: &gimli::DebugRanges<gimli::EndianBuf<Endianness>>,
        address_size: u8,
        base_address: u64,
    ) -> Result<Vec<gimli::Range>> {
        if let Some(range) = Self::parse_noncontiguous_ranges(
                entry, debug_ranges, address_size, base_address)? {
            Ok(range)
        } else if let Some(range) = Self::parse_contiguous_range(entry)?
                .map(|range| vec![range]) {
            Ok(range)
        } else {
            Ok(vec![])
        }
    }

    fn parse_noncontiguous_ranges(
        entry: &gimli::DebuggingInformationEntry<gimli::EndianBuf<Endianness>>,
        debug_ranges: &gimli::DebugRanges<gimli::EndianBuf<Endianness>>,
        address_size: u8,
        base_address: u64,
    ) -> Result<Option<Vec<gimli::Range>>>
    {
        let offset = match entry.attr_value(gimli::DW_AT_ranges) {
            Ok(Some(gimli::AttributeValue::DebugRangesRef(offset))) => offset,
            Err(e) => {
                return Err(Error::from(e)).chain_err(|| err("invalid ranges attribute"));
            }
            _ => return Ok(None),
        };

        let ranges = debug_ranges
            .ranges(offset, address_size, base_address)
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

    fn get_entry<'a>(
        entry: &gimli::DebuggingInformationEntry<'a, 'a, gimli::EndianBuf<'input, Endianness>>,
        header: &'a gimli::CompilationUnitHeader<gimli::EndianBuf<'input, Endianness>>,
        abbrev: &'a gimli::Abbreviations,
        attr: gimli::DwAt,
    ) -> Result<Option<gimli::DebuggingInformationEntry<'a, 'a, gimli::EndianBuf<'input, Endianness>>>> {
        let offset = match entry.attr_value(attr)? {
            Some(gimli::AttributeValue::UnitRef(offset)) => {
                offset
            }
            Some(gimli::AttributeValue::DebugInfoRef(offset)) => {
                if let Some(unit_offset) = offset.to_unit_offset(header) {
                    unit_offset
                } else {
                    // is this happening in real life?  This would require us to
                    // either parse other stuff again or cache all units.
                    return Ok(None);
                }
            }
            None => {
                return Ok(None);
            }
            _ => {
                // XXX: sadly there is probably more that can come back here
                return Ok(None);
            }
        };

        let mut entries = header.entries_at_offset(abbrev, offset)?;
        let (_, entry) = entries
            .next_dfs()?
            .ok_or_else(|| {
                err("invalid debug symbols: dangling entry offset")
            })?;
        Ok(Some(entry.clone()))
    }

    fn resolve_function_name<'a, 'b>(
        entry: &gimli::DebuggingInformationEntry<'a, 'b, gimli::EndianBuf<'input, Endianness>>,
        header: &gimli::CompilationUnitHeader<gimli::EndianBuf<'input, Endianness>>,
        debug_str: &gimli::DebugStr<gimli::EndianBuf<'input, Endianness>>,
        abbrev: &gimli::Abbreviations,
    ) -> Result<Option<gimli::EndianBuf<'input, Endianness>>> {

        // For naming, we prefer the linked name, if available
        if let Some(name) = entry
            .attr(gimli::DW_AT_linkage_name)
            .map_err(|e| Error::from(e))
            .chain_err(|| err("invalid subprogram linkage name"))?
            .and_then(|attr| attr.string_value(debug_str))
        {
            return Ok(Some(name));
        }
        if let Some(name) = entry
            .attr(gimli::DW_AT_MIPS_linkage_name)
            .map_err(|e| Error::from(e))
            .chain_err(|| err("invalid subprogram linkage name"))?
            .and_then(|attr| attr.string_value(debug_str))
        {
            return Ok(Some(name));
        }

        // Linked name is not available, so fall back to just plain old name, if that's available.
        if let Some(name) = entry
            .attr(gimli::DW_AT_name)
            .map_err(|e| Error::from(e))
            .chain_err(|| err("invalid subprogram name"))?
            .and_then(|attr| attr.string_value(debug_str))
        {
            return Ok(Some(name));
        }

        // If we don't have the link name, check if this function refers to another
        if let Some(abstract_origin) =
            Self::get_entry(entry, header, abbrev, gimli::DW_AT_abstract_origin)
                .chain_err(|| err("invalid subprogram abstract origin"))?
        {
            let name = Self::resolve_function_name(&abstract_origin, header, debug_str, abbrev)
                .chain_err(|| err("abstract origin does not resolve to a name"))?;
            return Ok(name);
        }
        if let Some(specification) =
            Self::get_entry(entry, header, abbrev, gimli::DW_AT_specification)
                .chain_err(|| err("invalid subprogram specification"))?
        {
            let name = Self::resolve_function_name(&specification, header, debug_str, abbrev)
                .chain_err(|| err("specification does not resolve to a name"))?;
            return Ok(name);
        }

        Ok(None)
    }
}

#[derive(Debug)]
struct Lines<'input> {
    sequences: Vec<Sequence>,
    header: gimli::LineNumberProgramHeader<gimli::EndianBuf<'input, Endianness>>,
}

impl<'input> Lines<'input> {
    fn new(
        debug_line: &gimli::DebugLine<gimli::EndianBuf<'input, Endianness>>,
        line_offset: gimli::DebugLineOffset,
        address_size: u8,
        comp_dir: Option<gimli::EndianBuf<'input, Endianness>>,
        comp_name: Option<gimli::EndianBuf<'input, Endianness>>,
    ) -> Result<Self> {
        let program = debug_line
            .program(line_offset, address_size, comp_dir, comp_name)?;
        let mut sequences = vec![];
        let mut sequence_rows: Vec<Row> = vec![];
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
                    sequences.push(Sequence {
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
                    sequence_rows.push(Row {
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
            sequences.push(Sequence {
                low_address,
                high_address,
                rows: sequence_rows,
            });
        }
        // Sort so we can binary search.
        sequences.sort_by(|a, b| a.low_address.cmp(&b.low_address));

        Ok(Lines {
            sequences: sequences,
            header: header,
        })
    }

    pub fn get_rows(&self, rng: &gimli::Range) -> &[Row] {
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

#[derive(Debug)]
struct Sequence {
    low_address: u64,
    high_address: u64,
    rows: Vec<Row>,
}

#[derive(Debug, PartialEq, Eq)]
struct Row {
    address: u64,
    file_index: u64,
    line: Option<u64>,
}
