use std::mem;
use std::io::Write;
use std::cmp::Ordering;

use symbolic_common::{Error, ErrorKind, Result, ResultExt, Endianness};
use symbolic_debuginfo::{Object, DwarfSection};

use gimli;

fn err(msg: &'static str) -> Error {
    Error::from(ErrorKind::BadDwarfData(msg))
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
            ($sect:ident) => {{
                let sect = obj.get_dwarf_section(DwarfSection::$sect)
                    .ok_or(ErrorKind::MissingSection(
                        DwarfSection::$sect.get_elf_section()))?;
                gimli::$sect::new(sect.as_bytes(), obj.endianess())
            }}
        }

        let debug_info = section!(DebugInfo);
        let debug_abbrev = section!(DebugAbbrev);
        let debug_line = section!(DebugLine);
        let debug_ranges = section!(DebugRanges);
        let debug_str = section!(DebugStr);

        let mut headers = debug_info.units();

        while let Some(header) = headers.next()
                .chain_err(|| err("couldn't get DIE header"))? {
            let unit_opt = Unit::try_parse(
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
            panic!("{:#?}", unit);
        }

        Ok(())
    }
}


#[derive(Debug)]
struct Unit<'input> {
    range: Option<gimli::Range>,
    lines: Lines,
    comp_dir: Option<gimli::EndianBuf<'input, Endianness>>,
    programs: Vec<Program<'input>>,
    language: Option<gimli::DwLang>,
}

impl<'input> Unit<'input> {
    fn try_parse(
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
        let mut unit = {
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

            // Where does our compilation unit live?
            let range = Self::parse_contiguous_range(entry)
                .chain_err(|| "compilation unit has invalid low_pc and/or high_pc")?;

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
            let comp_dir = entry
                .attr(gimli::DW_AT_comp_dir)
                .map_err(|e| Error::from(e))
                .chain_err(|| err("invalid compilation unit directory"))?
                .and_then(|attr| attr.string_value(debug_str));
            let comp_name = entry
                .attr(gimli::DW_AT_name)
                .map_err(|e| Error::from(e))
                .chain_err(|| err("invalid compilation unit name"))?
                .and_then(|attr| attr.string_value(debug_str));
            let language = entry
                .attr(gimli::DW_AT_language)
                .map_err(|e| Error::from(e))?
                .and_then(|attr| match attr.value() {
                    gimli::AttributeValue::Language(lang) => Some(lang),
                    _ => None,
                });

            let lines = Lines::new(
                debug_line,
                line_offset,
                header.address_size(),
                comp_dir,
                comp_name,
            )?;

            Unit {
                range: range,
                lines: lines,
                comp_dir,
                programs: vec![],
                language: language,
            }
        };        

        Ok(Some(unit))
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
                return Err(Error::from(e))
                    .chain_err(|| err("invalid high_pc attribute"))
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
}

#[derive(Debug)]
struct Program<'input> {
    ranges: Vec<gimli::Range>,
    name: gimli::EndianBuf<'input, Endianness>,
    inlined: bool,
}

impl<'input> Program<'input> {
    fn contains_address(&self, address: u64) -> bool {
        self.ranges
            .iter()
            .any(|range| address >= range.begin && address < range.end)
    }
}

#[derive(Debug)]
struct Lines {
    sequences: Vec<Sequence>,
}

impl Lines {
    fn new<'input>(
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
        })
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
