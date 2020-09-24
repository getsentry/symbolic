//! Support for DWARF debugging information, common to ELF and MachO.
//!
//! The central element of this module is the [`Dwarf`] trait, which is implemented by [`ElfObject`]
//! and [`MachObject`]. The dwarf debug session object can be obtained via getters on those types.
//!
//! [`Dwarf`]: trait.Dwarf.html
//! [`ElfObject`]: ../elf/struct.ElfObject.html
//! [`MachObject`]: ../macho/struct.MachObject.html

use std::borrow::Cow;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;

use fallible_iterator::FallibleIterator;
use gimli::read::{AttributeValue, Range};
use gimli::{constants, UnitSectionOffset};
use lazycell::LazyCell;
use thiserror::Error;

use symbolic_common::{AsSelf, Language, Name, SelfCell};

use crate::base::*;
use crate::private::FunctionStack;

#[doc(hidden)]
pub use gimli;
pub use gimli::read::Error as GimliError;
pub use gimli::RunTimeEndian as Endian;

type Slice<'a> = gimli::read::EndianSlice<'a, Endian>;
type RangeLists<'a> = gimli::read::RangeLists<Slice<'a>>;
type Unit<'a> = gimli::read::Unit<Slice<'a>>;
type DwarfInner<'a> = gimli::read::Dwarf<Slice<'a>>;

type Die<'d, 'u> = gimli::read::DebuggingInformationEntry<'u, 'u, Slice<'d>, usize>;
type Attribute<'a> = gimli::read::Attribute<Slice<'a>>;
type UnitOffset = gimli::read::UnitOffset<usize>;
type DebugInfoOffset = gimli::DebugInfoOffset<usize>;

type CompilationUnitHeader<'a> = gimli::read::CompilationUnitHeader<Slice<'a>>;
type IncompleteLineNumberProgram<'a> = gimli::read::IncompleteLineProgram<Slice<'a>>;
type LineNumberProgramHeader<'a> = gimli::read::LineProgramHeader<Slice<'a>>;
type LineProgramFileEntry<'a> = gimli::read::FileEntry<Slice<'a>>;

/// An error handling [`DWARF`](trait.Dwarf.html) debugging information.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum DwarfError {
    /// A compilation unit referenced by index does not exist.
    #[error("compilation unit for offset {0} does not exist")]
    InvalidUnitRef(usize),

    /// A file record referenced by index does not exist.
    #[error("referenced file {0} does not exist")]
    InvalidFileRef(u64),

    /// An inline record was encountered without an inlining parent.
    #[error("unexpected inline function without parent")]
    UnexpectedInline,

    /// The debug_ranges of a function are invalid.
    #[error("function with inverted address range")]
    InvertedFunctionRange,

    /// The DWARF file is corrupted. See the cause for more information.
    #[error("corrupted dwarf debug data")]
    CorruptedData(#[from] GimliError),
}

/// DWARF section information including its data.
///
/// This is returned from objects implementing the [`Dwarf`] trait.
///
/// [`Dwarf`]: trait.Dwarf.html
#[derive(Clone)]
pub struct DwarfSection<'data> {
    /// Memory address of this section in virtual memory.
    pub address: u64,

    /// File offset of this section.
    pub offset: u64,

    /// Section address alignment (power of two).
    pub align: u64,

    /// Binary data of this section.
    pub data: Cow<'data, [u8]>,
}

impl fmt::Debug for DwarfSection<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DwarfSection")
            .field("address", &format_args!("{:#x}", self.address))
            .field("offset", &format_args!("{:#x}", self.offset))
            .field("align", &format_args!("{:#x}", self.align))
            .field("len()", &self.data.len())
            .finish()
    }
}

/// Provides access to DWARF debugging information independent of the container file type.
///
/// When implementing this trait, verify whether the container file type supports compressed section
/// data. If so, override the provided `section_data` method. Also, if there is a faster way to
/// check for the existence of a section without loading its data, override `has_section`.
pub trait Dwarf<'data> {
    /// Returns whether the file was compiled for a big-endian or little-endian machine.
    ///
    /// This can usually be determined by inspecting the file's headers. Sometimes, this is also
    /// given by the architecture.
    fn endianity(&self) -> Endian;

    /// Returns information and raw data of a section.
    ///
    /// The section name is given without leading punctuation, such dots or underscores. For
    /// instance, the name of the Debug Info section would be `"debug_info"`, which translates to
    /// `".debug_info"` in ELF and `"__debug_info"` in MachO.
    ///
    /// Certain containers might allow compressing section data. In this case, this function returns
    /// the compressed data. To get uncompressed data instead, use `section_data`.
    fn raw_section(&self, name: &str) -> Option<DwarfSection<'data>>;

    /// Returns information and data of a section.
    ///
    /// If the section is compressed, this decompresses on the fly and returns allocated memory.
    /// Otherwise, this should return a slice of the raw data.
    ///
    /// The section name is given without leading punctuation, such dots or underscores. For
    /// instance, the name of the Debug Info section would be `"debug_info"`, which translates to
    /// `".debug_info"` in ELF and `"__debug_info"` in MachO.
    fn section(&self, name: &str) -> Option<DwarfSection<'data>> {
        self.raw_section(name)
    }

    /// Determines whether the specified section exists.
    ///
    /// The section name is given without leading punctuation, such dots or underscores. For
    /// instance, the name of the Debug Info section would be `"debug_info"`, which translates to
    /// `".debug_info"` in ELF and `"__debug_info"` in MachO.
    fn has_section(&self, name: &str) -> bool {
        self.raw_section(name).is_some()
    }
}

/// A row in the DWARF line program.
#[derive(Debug)]
struct DwarfRow {
    address: u64,
    file_index: u64,
    line: Option<u64>,
    size: Option<u64>,
}

/// A sequence in the DWARF line program.
#[derive(Debug)]
struct DwarfSequence {
    start: u64,
    end: u64,
    rows: Vec<DwarfRow>,
}

/// Helper that prepares a DwarfLineProgram for more efficient access.
#[derive(Debug)]
struct DwarfLineProgram<'d> {
    header: LineNumberProgramHeader<'d>,
    sequences: Vec<DwarfSequence>,
}

impl<'d, 'a> DwarfLineProgram<'d> {
    fn prepare(program: IncompleteLineNumberProgram<'d>) -> Result<Self, DwarfError> {
        let mut sequences = Vec::new();
        let mut sequence_rows = Vec::<DwarfRow>::new();
        let mut prev_address = 0;
        let mut state_machine = program.rows();

        while let Ok(Some((_, &program_row))) = state_machine.next_row() {
            let address = program_row.address();

            if let Some(last_row) = sequence_rows.last_mut() {
                if address >= last_row.address {
                    last_row.size = Some(address - last_row.address);
                }
            }

            if program_row.end_sequence() {
                // Theoretically, there could be multiple DW_LNE_end_sequence in a row. We're not
                // interested in empty sequences, so we can skip them completely.
                if !sequence_rows.is_empty() {
                    sequences.push(DwarfSequence {
                        start: sequence_rows[0].address,
                        // Take a defensive approach and ensure that `high_address` always covers
                        // the last encountered row, assuming a 1 byte instruction.
                        end: if address < prev_address {
                            prev_address + 1
                        } else {
                            address
                        },
                        rows: sequence_rows.drain(..).collect(),
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
                        size: None,
                    });
                }
                prev_address = address;
            }
        }

        if !sequence_rows.is_empty() {
            // A sequence without an end_sequence row.
            // Let's assume the last row covered 1 byte.
            let start = sequence_rows[0].address;
            let end = prev_address + 1;
            sequences.push(DwarfSequence {
                start,
                end,
                rows: sequence_rows,
            });
        }

        // Sequences are not guaranteed to be in order.
        dmsort::sort_by_key(&mut sequences, |x| x.start);

        Ok(DwarfLineProgram {
            header: state_machine.header().clone(),
            sequences,
        })
    }

    pub fn get_rows(&self, range: &Range) -> &[DwarfRow] {
        for seq in &self.sequences {
            if seq.end <= range.begin || seq.start > range.end {
                continue;
            }

            let from = match seq.rows.binary_search_by_key(&range.begin, |x| x.address) {
                Ok(idx) => idx,
                Err(0) => continue,
                Err(next_idx) => next_idx - 1,
            };

            let len = seq.rows[from..]
                .binary_search_by_key(&range.end, |x| x.address)
                .unwrap_or_else(|e| e);
            return &seq.rows[from..from + len];
        }
        &[]
    }
}

/// A slim wrapper around a DWARF unit.
#[derive(Clone, Copy, Debug)]
struct UnitRef<'d, 'a> {
    info: &'a DwarfInfo<'d>,
    unit: &'a Unit<'d>,
}

impl<'d, 'a> UnitRef<'d, 'a> {
    /// Resolve the binary value of an attribute.
    #[inline(always)]
    fn slice_value(&self, value: AttributeValue<Slice<'d>>) -> Option<&'d [u8]> {
        self.info
            .attr_string(self.unit, value)
            .map(|reader| reader.slice())
            .ok()
    }

    /// Resolve the actual string value of an attribute.
    #[inline(always)]
    fn string_value(&self, value: AttributeValue<Slice<'d>>) -> Option<Cow<'d, str>> {
        let slice = self.slice_value(value)?;
        Some(String::from_utf8_lossy(slice))
    }

    /// Resolves an entry and if found invokes a function to transform it.
    ///
    /// As this might resolve into cached information the data borrowed from
    /// abbrev can only be temporarily accessed in the callback.
    fn resolve_reference<T, F>(&self, attr: Attribute<'d>, f: F) -> Result<Option<T>, DwarfError>
    where
        F: FnOnce(UnitRef<'d, '_>, &Die<'d, '_>) -> Result<Option<T>, DwarfError>,
    {
        let (unit, offset) = match attr.value() {
            AttributeValue::UnitRef(offset) => (*self, offset),
            AttributeValue::DebugInfoRef(offset) => self.info.find_unit_offset(offset)?,
            // TODO: There is probably more that can come back here.
            _ => return Ok(None),
        };

        let mut entries = unit.unit.entries_at_offset(offset)?;
        entries.next_entry()?;

        if let Some(entry) = entries.current() {
            f(unit, entry)
        } else {
            Ok(None)
        }
    }

    /// Resolves the function name of a debug entry.
    fn resolve_function_name(
        &self,
        entry: &Die<'d, '_>,
    ) -> Result<Option<Cow<'d, str>>, DwarfError> {
        let mut attrs = entry.attrs();
        let mut fallback_name = None;
        let mut reference_target = None;

        while let Some(attr) = attrs.next()? {
            match attr.name() {
                // Prioritize these. If we get them, take them.
                constants::DW_AT_linkage_name | constants::DW_AT_MIPS_linkage_name => {
                    return Ok(self.string_value(attr.value()));
                }
                constants::DW_AT_name => {
                    fallback_name = Some(attr);
                }
                constants::DW_AT_abstract_origin | constants::DW_AT_specification => {
                    reference_target = Some(attr);
                }
                _ => {}
            }
        }

        if let Some(attr) = fallback_name {
            return Ok(self.string_value(attr.value()));
        }

        if let Some(attr) = reference_target {
            let resolved = self.resolve_reference(attr, |ref_unit, ref_entry| {
                if self.unit.offset != ref_unit.unit.offset || entry.offset() != ref_entry.offset()
                {
                    ref_unit.resolve_function_name(ref_entry)
                } else {
                    Ok(None)
                }
            })?;

            if let Some(name) = resolved {
                return Ok(Some(name));
            }
        }

        Ok(None)
    }
}

/// Wrapper around a DWARF Unit.
#[derive(Debug)]
struct DwarfUnit<'d, 'a> {
    inner: UnitRef<'d, 'a>,
    language: Language,
    line_program: Option<DwarfLineProgram<'d>>,
}

impl<'d, 'a> DwarfUnit<'d, 'a> {
    /// Creates a DWARF unit from the gimli `Unit` type.
    fn from_unit(unit: &'a Unit<'d>, info: &'a DwarfInfo<'d>) -> Result<Option<Self>, DwarfError> {
        let mut entries = unit.entries();
        let entry = match entries.next_dfs()? {
            Some((_, entry)) => entry,
            None => return Err(gimli::read::Error::MissingUnitDie.into()),
        };

        // Clang's LLD might eliminate an entire compilation unit and simply set the low_pc to zero
        // and remove all range entries to indicate that it is missing. Skip such a unit, as it does
        // not contain any code that can be executed. Special case relocatable objects, as here the
        // range information has not been written yet and all units look like this.
        if info.kind != ObjectKind::Relocatable
            && unit.low_pc == 0
            && entry.attr(constants::DW_AT_ranges)?.is_none()
        {
            return Ok(None);
        }

        let language = match entry.attr_value(constants::DW_AT_language)? {
            Some(AttributeValue::Language(lang)) => language_from_dwarf(lang),
            _ => Language::Unknown,
        };

        let line_program = match unit.line_program {
            Some(ref program) => Some(DwarfLineProgram::prepare(program.clone())?),
            None => None,
        };

        Ok(Some(DwarfUnit {
            inner: UnitRef { info, unit },
            language,
            line_program,
        }))
    }

    /// The path of the compilation directory. File names are usually relative to this path.
    fn compilation_dir(&self) -> &'d [u8] {
        match self.inner.unit.comp_dir {
            Some(ref dir) => dir.slice(),
            None => &[],
        }
    }

    /// Parses the call site and range lists of this Debugging Information Entry.
    fn parse_ranges(
        &self,
        entry: &Die<'d, '_>,
        range_buf: &mut Vec<Range>,
    ) -> Result<(Option<u64>, Option<u64>), DwarfError> {
        let mut tuple = (None, None);
        let mut low_pc = None;
        let mut high_pc = None;
        let mut high_pc_rel = None;

        let mut attrs = entry.attrs();
        while let Some(attr) = attrs.next()? {
            match attr.name() {
                constants::DW_AT_low_pc => match attr.value() {
                    AttributeValue::Addr(addr) => low_pc = Some(addr),
                    _ => unreachable!(),
                },
                constants::DW_AT_high_pc => match attr.value() {
                    AttributeValue::Addr(addr) => high_pc = Some(addr),
                    AttributeValue::Udata(size) => high_pc_rel = Some(size),
                    _ => unreachable!(),
                },
                constants::DW_AT_call_line => match attr.value() {
                    AttributeValue::Udata(line) => tuple.0 = Some(line),
                    _ => unreachable!(),
                },
                constants::DW_AT_call_file => match attr.value() {
                    AttributeValue::FileIndex(file) => tuple.1 = Some(file),
                    _ => unreachable!(),
                },
                constants::DW_AT_ranges
                | constants::DW_AT_rnglists_base
                | constants::DW_AT_start_scope => {
                    match self.inner.info.attr_ranges(self.inner.unit, attr.value())? {
                        Some(mut ranges) => {
                            while let Some(range) = ranges.next()? {
                                range_buf.push(range);
                            }
                        }
                        None => continue,
                    }
                }
                _ => continue,
            }
        }

        // Found DW_AT_ranges, so early-exit here
        if !range_buf.is_empty() {
            return Ok(tuple);
        }

        // To go by the logic in dwarf2read, a `low_pc` of 0 can indicate an
        // eliminated duplicate when the GNU linker is used. In relocatable
        // objects, all functions are at `0` since they have not been placed
        // yet, so we want to retain them.
        let kind = self.inner.info.kind;
        let low_pc = match low_pc {
            Some(low_pc) if low_pc != 0 || kind == ObjectKind::Relocatable => low_pc,
            _ => return Ok(tuple),
        };

        let high_pc = match (high_pc, high_pc_rel) {
            (Some(high_pc), _) => high_pc,
            (_, Some(high_pc_rel)) => low_pc.wrapping_add(high_pc_rel),
            _ => return Ok(tuple),
        };

        if low_pc == high_pc {
            // most likely low_pc == high_pc means the DIE should be ignored.
            // https://sourceware.org/ml/gdb-patches/2011-03/msg00739.html
            return Ok(tuple);
        }

        if low_pc > high_pc {
            // TODO: consider swallowing errors here?
            return Err(DwarfError::InvertedFunctionRange);
        }

        range_buf.push(Range {
            begin: low_pc,
            end: high_pc,
        });

        Ok(tuple)
    }

    /// Resolves line records of a DIE's range list and puts them into the given buffer.
    fn resolve_lines(&self, ranges: &[Range]) -> Vec<LineInfo<'d>> {
        // Early exit in case this unit did not declare a line program.
        let line_program = match self.line_program {
            Some(ref program) => program,
            None => return Vec::new(),
        };

        let mut lines = Vec::new();
        for range in ranges {
            // Most of the rows will result in a line record. Reserve the number of rows in the line
            // record to avoid frequent reallocations when adding a large number of lines in the
            // beginning.
            let rows = line_program.get_rows(range);
            lines.reserve(rows.len());

            // Suppose we've a range [0x50; 0x100) and in sequences, we've:
            //  - [0x25; 0x60) -> l.12, f.34
            //  - [0x60; 0x80) -> l.13, f.34
            //  - [0x80; 0x120) -> l.14, f.34
            // So for this range, we'll get exactly the 3 above rows
            // and we need:
            // - to fix the address of the 1st row to 0x50
            // - to do nothing on the 2nd since it's fully included in the range
            // - to fix the size of the last row to 0x20 (0x100 - 0x80)
            // At the end we exactly splited the initial range into 3 contiguous ranges
            // and each of them maps a different line.
            if let Some((first, rows)) = rows.split_first() {
                let mut last_file = first.file_index;
                let mut last_info = LineInfo {
                    address: range.begin - self.inner.info.load_address,
                    size: first.size.map(|s| s + first.address - range.begin),
                    file: self.resolve_file(first.file_index).unwrap_or_default(),
                    line: first.line.unwrap_or(0),
                };

                for row in rows {
                    let line = row.line.unwrap_or(0);

                    // We're in a range so we can collapse the lines without any side effects
                    if (last_file, last_info.line) == (row.file_index, line) {
                        // We collapse the lines but need to fix the last line size
                        if let Some(size) = last_info.size.as_mut() {
                            *size += row.size.unwrap_or(0);
                        }

                        continue;
                    }

                    // We've a new line/file so push the previous line_info
                    lines.push(last_info);

                    last_file = row.file_index;
                    last_info = LineInfo {
                        address: row.address - self.inner.info.load_address,
                        size: row.size,
                        file: self.resolve_file(row.file_index).unwrap_or_default(),
                        line,
                    };
                }

                // Fix the size of the last line
                if let Some(size) = last_info.size.as_mut() {
                    *size = range.end - self.inner.info.load_address - last_info.address;
                }

                lines.push(last_info);
            }
        }

        lines
    }

    /// Resolves file information from a line program.
    fn file_info(
        &self,
        line_program: &LineNumberProgramHeader<'d>,
        file: &LineProgramFileEntry<'d>,
    ) -> FileInfo<'d> {
        FileInfo {
            dir: file
                .directory(line_program)
                .and_then(|attr| self.inner.slice_value(attr))
                .unwrap_or_default(),
            name: self.inner.slice_value(file.path_name()).unwrap_or_default(),
        }
    }

    /// Resolves a file entry by its index.
    fn resolve_file(&self, file_id: u64) -> Option<FileInfo<'d>> {
        let line_program = match self.line_program {
            Some(ref program) => &program.header,
            None => return None,
        };

        line_program
            .file(file_id)
            .map(|file| self.file_info(line_program, file))
    }

    /// Collects all functions within this compilation unit.
    fn functions(&self, range_buf: &mut Vec<Range>) -> Result<Vec<Function<'d>>, DwarfError> {
        let mut depth = 0;
        let mut skipped_depth = None;
        let mut functions = Vec::new();

        let mut stack = FunctionStack::new();
        let mut entries = self.inner.unit.entries();
        while let Some((movement, entry)) = entries.next_dfs()? {
            depth += movement;

            // If we're navigating within a skipped function (see below), we can ignore this
            // entry completely. Otherwise, we've moved out of any skipped function and can
            // reset the stored depth.
            match skipped_depth {
                Some(skipped) if depth > skipped => continue,
                _ => skipped_depth = None,
            }

            // Flush all functions out that exceed the current iteration depth. Since we
            // encountered an entry at this level, there will be no more inlinees to the
            // previous function at the same level or any of it's children.
            stack.flush(depth, &mut functions);

            // Skip anything that is not a function.
            let inline = match entry.tag() {
                constants::DW_TAG_subprogram => false,
                constants::DW_TAG_inlined_subroutine => true,
                _ => continue,
            };

            range_buf.clear();
            let (call_line, call_file) = self.parse_ranges(entry, range_buf)?;

            // Ranges can be empty for two reasons: (1) the function is a no-op and does not
            // contain any code, or (2) the function did contain eliminated dead code. In the
            // latter case, a surrogate DIE remains with `DW_AT_low_pc(0)` and empty ranges.
            // That DIE might still contain inlined functions with actual ranges, which must all
            // be skipped.
            if range_buf.is_empty() {
                skipped_depth = Some(depth);
                continue;
            }

            // We have a non-inlined function which has two ranges or more, probably split because
            // of cold paths.
            if !inline && range_buf.len() != 1 {
                // TODO: Emit one function record per range, instead of skipping this function. This
                // also applies to PDB, where this is more common with LTO enabled.
                skipped_depth = Some(depth);
                continue;
            }

            let function_address = range_buf[0].begin - self.inner.info.load_address;
            let function_size = range_buf[range_buf.len() - 1].end - range_buf[0].begin;
            let function_end = function_address + function_size;

            // Resolve functions in the symbol table first. Only if there is no entry, fall back
            // to debug information only if there is no match. Sometimes, debug info contains a
            // lesser quality of symbol names.
            //
            // XXX: Maybe we should actually parse the ranges in the resolve function and always
            // look at the symbol table based on the start of the DIE range.
            let symbol_name = if !inline {
                self.inner
                    .info
                    .symbol_map
                    .lookup_range(function_address..function_end)
                    .and_then(|symbol| symbol.name.clone())
            } else {
                None
            };

            let name = match symbol_name {
                Some(name) => Some(name),
                None => self.inner.resolve_function_name(entry).ok().flatten(),
            };

            // Avoid constant allocations by collecting repeatedly into the same buffer and
            // draining the results out of it. This keeps the original buffer allocated and
            // allows for a single allocation per call to `resolve_lines`.
            let lines = self.resolve_lines(&range_buf);

            if inline {
                // An inlined function must always have a parent. An empty list of funcs
                // indicates invalid debug information.
                let parent = match stack.peek_mut() {
                    Some(parent) => parent,
                    None => return Err(DwarfError::UnexpectedInline),
                };

                // Make sure there is correct line information for the call site of this inlined
                // function. In general, a compiler should always output the call line and call file
                // for inlined subprograms. If this info is missing, the lookup might return invalid
                // line numbers.
                //
                // All the lines have been collected in the parent so just get the lines from the
                // parent which belong to each range in the inlinee.
                if let (Some(line), Some(file_id)) = (call_line, call_file) {
                    let file = self.resolve_file(file_id).unwrap_or_default();
                    let lines = &mut parent.lines;

                    let mut index = 0;
                    for range in range_buf.iter() {
                        let range_begin = range.begin - self.inner.info.load_address;
                        let range_end = range.end - self.inner.info.load_address;

                        // Check if there is a line record covering the start of this range,
                        // otherwise insert a new record pointing to the correct call location.
                        if let Some(next) = lines.get(index) {
                            if next.address > range_begin {
                                let line_info = LineInfo {
                                    address: range_begin,
                                    size: Some(range_end.min(next.address) - range_begin),
                                    file: file.clone(),
                                    line,
                                };

                                lines.insert(index, line_info);
                                index += 1;
                            }
                        }

                        while index < lines.len() {
                            let record = &mut lines[index];

                            // Advance to the next range since we're done here.
                            if record.address >= range_end {
                                break;
                            }

                            index += 1;

                            // Skip forward to the next line record that overlaps with our range.
                            // Lines before belong to the parent function or another inlinee.
                            let record_end = record.address + record.size.unwrap_or(0);
                            if record_end <= range_begin {
                                continue;
                            }

                            // Split the parent record if it exceeds the end of this range. We can
                            // assume that record.size is set here since we passed the previous
                            // condition.
                            let split = if record_end > range_end {
                                record.size = Some(range_end - record.address);

                                Some(LineInfo {
                                    address: range_end,
                                    size: Some(record_end - range_end),
                                    file: record.file.clone(),
                                    line: record.line,
                                })
                            } else {
                                None
                            };

                            if record.address < range_begin {
                                // Fix the length of this line record to go up to the start of the
                                // inline function. This effectively splits the previous record in
                                // two.
                                let max_size = range_begin - record.address;
                                if record.size.map_or(true, |prev_size| prev_size > max_size) {
                                    record.size = Some(max_size);
                                }

                                // For example: [0; 100) split around 20 will give [0; 20) and [20;
                                // 100) so the size of the second is 100 - 20
                                let size = record_end.min(range_end) - range_begin;

                                // Insert a new record pointing to the correct call location. Note
                                // that "base_dir" can be inherited safely here.
                                let line_info = LineInfo {
                                    address: range_begin,
                                    size: Some(size),
                                    file: file.clone(),
                                    line,
                                };

                                lines.insert(index, line_info);
                                index += 1;
                            } else {
                                record.file = file.clone();
                                record.line = line;
                            };

                            // Insert the split record after mutating the previous one to avoid
                            // borrowing issues. Do not skip it, since it may have to be split
                            // further.
                            if let Some(split) = split {
                                lines.insert(index, split);
                            }
                        }

                        // The range is not fully covered by the parent. Add a new record that
                        // covers the remaining part.
                        if let Some(prev) = index.checked_sub(1).and_then(|i| lines.get(i)) {
                            let record_end = prev.address + prev.size.unwrap_or(0);
                            if record_end < range_end {
                                let line_info = LineInfo {
                                    address: record_end,
                                    size: Some(range_end - record_end),
                                    file: file.clone(),
                                    line,
                                };

                                lines.insert(index, line_info);
                                index += 1;
                            }
                        }
                    }
                }
            }

            let function = Function {
                address: function_address,
                size: function_size,
                name: Name::with_language(name.unwrap_or_default(), self.language),
                compilation_dir: self.compilation_dir(),
                lines,
                inlinees: Vec::new(),
                inline,
            };

            stack.push(depth, function)
        }

        // We're done, flush the remaining stack.
        stack.flush(0, &mut functions);

        Ok(functions)
    }
}

/// Converts a DWARF language number into our `Language` type.
fn language_from_dwarf(language: gimli::DwLang) -> Language {
    match language {
        constants::DW_LANG_C => Language::C,
        constants::DW_LANG_C11 => Language::C,
        constants::DW_LANG_C89 => Language::C,
        constants::DW_LANG_C99 => Language::C,
        constants::DW_LANG_C_plus_plus => Language::Cpp,
        constants::DW_LANG_C_plus_plus_03 => Language::Cpp,
        constants::DW_LANG_C_plus_plus_11 => Language::Cpp,
        constants::DW_LANG_C_plus_plus_14 => Language::Cpp,
        constants::DW_LANG_D => Language::D,
        constants::DW_LANG_Go => Language::Go,
        constants::DW_LANG_ObjC => Language::ObjC,
        constants::DW_LANG_ObjC_plus_plus => Language::ObjCpp,
        constants::DW_LANG_Rust => Language::Rust,
        constants::DW_LANG_Swift => Language::Swift,
        _ => Language::Unknown,
    }
}

/// Data of a specific DWARF section.
struct DwarfSectionData<'data, S> {
    data: Cow<'data, [u8]>,
    endianity: Endian,
    _ph: PhantomData<S>,
}

impl<'data, S> DwarfSectionData<'data, S>
where
    S: gimli::read::Section<Slice<'data>>,
{
    /// Loads data for this section from the object file.
    fn load<D>(dwarf: &D) -> Self
    where
        D: Dwarf<'data>,
    {
        DwarfSectionData {
            data: dwarf
                .section(&S::section_name()[1..])
                .map(|section| section.data)
                .unwrap_or_default(),
            endianity: dwarf.endianity(),
            _ph: PhantomData,
        }
    }

    /// Creates a gimli dwarf section object from the loaded data.
    fn to_gimli(&'data self) -> S {
        S::from(Slice::new(&self.data, self.endianity))
    }
}

impl<'d, S> fmt::Debug for DwarfSectionData<'d, S>
where
    S: gimli::read::Section<Slice<'d>>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let owned = match self.data {
            Cow::Owned(_) => true,
            Cow::Borrowed(_) => false,
        };

        f.debug_struct("DwarfSectionData")
            .field("type", &S::section_name())
            .field("endianity", &self.endianity)
            .field("len()", &self.data.len())
            .field("owned()", &owned)
            .finish()
    }
}

/// All DWARF sections that are needed by `DwarfDebugSession`.
struct DwarfSections<'data> {
    debug_abbrev: DwarfSectionData<'data, gimli::read::DebugAbbrev<Slice<'data>>>,
    debug_info: DwarfSectionData<'data, gimli::read::DebugInfo<Slice<'data>>>,
    debug_line: DwarfSectionData<'data, gimli::read::DebugLine<Slice<'data>>>,
    debug_line_str: DwarfSectionData<'data, gimli::read::DebugLineStr<Slice<'data>>>,
    debug_str: DwarfSectionData<'data, gimli::read::DebugStr<Slice<'data>>>,
    debug_str_offsets: DwarfSectionData<'data, gimli::read::DebugStrOffsets<Slice<'data>>>,
    debug_ranges: DwarfSectionData<'data, gimli::read::DebugRanges<Slice<'data>>>,
    debug_rnglists: DwarfSectionData<'data, gimli::read::DebugRngLists<Slice<'data>>>,
}

impl<'data> DwarfSections<'data> {
    /// Loads all sections from a DWARF object.
    fn from_dwarf<D>(dwarf: &D) -> Result<Self, DwarfError>
    where
        D: Dwarf<'data>,
    {
        Ok(DwarfSections {
            debug_abbrev: DwarfSectionData::load(dwarf),
            debug_info: DwarfSectionData::load(dwarf),
            debug_line: DwarfSectionData::load(dwarf),
            debug_line_str: DwarfSectionData::load(dwarf),
            debug_str: DwarfSectionData::load(dwarf),
            debug_str_offsets: DwarfSectionData::load(dwarf),
            debug_ranges: DwarfSectionData::load(dwarf),
            debug_rnglists: DwarfSectionData::load(dwarf),
        })
    }
}

struct DwarfInfo<'data> {
    inner: DwarfInner<'data>,
    headers: Vec<CompilationUnitHeader<'data>>,
    units: Vec<LazyCell<Option<Unit<'data>>>>,
    symbol_map: SymbolMap<'data>,
    load_address: u64,
    kind: ObjectKind,
}

impl<'d> Deref for DwarfInfo<'d> {
    type Target = DwarfInner<'d>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'d> DwarfInfo<'d> {
    /// Parses DWARF information from its raw section data.
    pub fn parse(
        sections: &'d DwarfSections<'d>,
        symbol_map: SymbolMap<'d>,
        load_address: u64,
        kind: ObjectKind,
    ) -> Result<Self, DwarfError> {
        let inner = gimli::read::Dwarf {
            debug_abbrev: sections.debug_abbrev.to_gimli(),
            debug_addr: Default::default(),
            debug_info: sections.debug_info.to_gimli(),
            debug_line: sections.debug_line.to_gimli(),
            debug_line_str: sections.debug_line_str.to_gimli(),
            debug_str: sections.debug_str.to_gimli(),
            debug_str_offsets: sections.debug_str_offsets.to_gimli(),
            debug_str_sup: Default::default(),
            debug_types: Default::default(),
            locations: Default::default(),
            ranges: RangeLists::new(
                sections.debug_ranges.to_gimli(),
                sections.debug_rnglists.to_gimli(),
            ),
        };

        // Prepare random access to unit headers.
        let headers = inner.units().collect::<Vec<_>>()?;
        let units = headers.iter().map(|_| LazyCell::new()).collect();

        Ok(DwarfInfo {
            inner,
            headers,
            units,
            symbol_map,
            load_address,
            kind,
        })
    }

    /// Loads a compilation unit.
    fn get_unit(&self, index: usize) -> Result<Option<&Unit<'d>>, DwarfError> {
        // Silently ignore unit references out-of-bound
        let cell = match self.units.get(index) {
            Some(cell) => cell,
            None => return Ok(None),
        };

        let unit_opt = cell.try_borrow_with(|| {
            // Parse the compilation unit from the header. This requires a top-level DIE that
            // describes the unit itself. For some older DWARF files, this DIE might be missing
            // which causes gimli to error out. We prefer to skip them silently as this simply marks
            // an empty unit for us.
            let header = self.headers[index];
            match self.inner.unit(header) {
                Ok(unit) => Ok(Some(unit)),
                Err(gimli::read::Error::MissingUnitDie) => Ok(None),
                Err(error) => Err(DwarfError::from(error)),
            }
        })?;

        Ok(unit_opt.as_ref())
    }

    /// Resolves an offset into a different compilation unit.
    fn find_unit_offset(
        &self,
        offset: DebugInfoOffset,
    ) -> Result<(UnitRef<'d, '_>, UnitOffset), DwarfError> {
        let search_result = self
            .headers
            .binary_search_by_key(&offset, CompilationUnitHeader::offset);

        let index = match search_result {
            Ok(index) => index,
            Err(0) => return Err(DwarfError::InvalidUnitRef(offset.0)),
            Err(next_index) => next_index - 1,
        };

        if let Some(unit) = self.get_unit(index)? {
            let offset = UnitSectionOffset::DebugInfoOffset(offset);
            if let Some(unit_offset) = offset.to_unit_offset(unit) {
                return Ok((UnitRef { unit, info: self }, unit_offset));
            }
        }

        Err(DwarfError::InvalidUnitRef(offset.0))
    }

    /// Returns an iterator over all compilation units.
    fn units(&'d self) -> DwarfUnitIterator<'_> {
        DwarfUnitIterator {
            info: self,
            index: 0,
        }
    }
}

impl<'slf, 'd: 'slf> AsSelf<'slf> for DwarfInfo<'d> {
    type Ref = DwarfInfo<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        unsafe { std::mem::transmute(self) }
    }
}

impl fmt::Debug for DwarfInfo<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DwarfInfo")
            .field("headers", &self.headers)
            .field("symbol_map", &self.symbol_map)
            .field("load_address", &self.load_address)
            .finish()
    }
}

/// An iterator over compilation units in a DWARF object.
struct DwarfUnitIterator<'s> {
    info: &'s DwarfInfo<'s>,
    index: usize,
}

impl<'s> Iterator for DwarfUnitIterator<'s> {
    type Item = Result<DwarfUnit<'s, 's>, DwarfError>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.info.headers.len() {
            let result = self.info.get_unit(self.index);
            self.index += 1;

            let unit = match result {
                Ok(Some(unit)) => unit,
                Ok(None) => continue,
                Err(error) => return Some(Err(error)),
            };

            match DwarfUnit::from_unit(unit, self.info) {
                Ok(Some(unit)) => return Some(Ok(unit)),
                Ok(None) => continue,
                Err(error) => return Some(Err(error)),
            }
        }

        None
    }
}

impl std::iter::FusedIterator for DwarfUnitIterator<'_> {}

/// A debugging session for DWARF debugging information.
pub struct DwarfDebugSession<'data> {
    cell: SelfCell<Box<DwarfSections<'data>>, DwarfInfo<'data>>,
}

impl<'d> DwarfDebugSession<'d> {
    /// Parses a dwarf debugging information from the given DWARF file.
    pub fn parse<D>(
        dwarf: &D,
        symbol_map: SymbolMap<'d>,
        load_address: u64,
        kind: ObjectKind,
    ) -> Result<Self, DwarfError>
    where
        D: Dwarf<'d>,
    {
        let sections = DwarfSections::from_dwarf(dwarf)?;
        let cell = SelfCell::try_new(Box::new(sections), |sections| {
            DwarfInfo::parse(unsafe { &*sections }, symbol_map, load_address, kind)
        })?;

        Ok(DwarfDebugSession { cell })
    }

    /// Returns an iterator over all source files in this debug file.
    pub fn files(&self) -> DwarfFileIterator<'_> {
        DwarfFileIterator {
            units: self.cell.get().units(),
            files: DwarfUnitFileIterator::default(),
            finished: false,
        }
    }

    /// Returns an iterator over all functions in this debug file.
    pub fn functions(&self) -> DwarfFunctionIterator<'_> {
        DwarfFunctionIterator {
            units: self.cell.get().units(),
            functions: Vec::new().into_iter(),
            range_buf: Vec::new(),
            finished: false,
        }
    }

    /// Looks up a file's source contents by its full canonicalized path.
    ///
    /// The given path must be canonicalized.
    pub fn source_by_path(&self, _path: &str) -> Result<Option<Cow<'_, str>>, DwarfError> {
        Ok(None)
    }
}

impl<'d> DebugSession for DwarfDebugSession<'d> {
    type Error = DwarfError;

    fn functions(&self) -> DynIterator<'_, Result<Function<'_>, Self::Error>> {
        Box::new(self.functions())
    }

    fn files(&self) -> DynIterator<'_, Result<FileEntry<'_>, Self::Error>> {
        Box::new(self.files())
    }

    fn source_by_path(&self, path: &str) -> Result<Option<Cow<'_, str>>, Self::Error> {
        self.source_by_path(path)
    }
}

#[derive(Debug, Default)]
struct DwarfUnitFileIterator<'s> {
    unit: Option<DwarfUnit<'s, 's>>,
    index: usize,
}

impl<'s> Iterator for DwarfUnitFileIterator<'s> {
    type Item = FileEntry<'s>;

    fn next(&mut self) -> Option<Self::Item> {
        let unit = self.unit.as_ref()?;
        let line_program = unit.line_program.as_ref().map(|p| &p.header)?;
        let file = line_program.file_names().get(self.index)?;

        self.index += 1;

        Some(FileEntry {
            compilation_dir: unit.compilation_dir(),
            info: unit.file_info(line_program, file),
        })
    }
}

/// An iterator over source files in a DWARF file.
pub struct DwarfFileIterator<'s> {
    units: DwarfUnitIterator<'s>,
    files: DwarfUnitFileIterator<'s>,
    finished: bool,
}

impl<'s> Iterator for DwarfFileIterator<'s> {
    type Item = Result<FileEntry<'s>, DwarfError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        loop {
            if let Some(file_entry) = self.files.next() {
                return Some(Ok(file_entry));
            }

            let unit = match self.units.next() {
                Some(Ok(unit)) => unit,
                Some(Err(error)) => return Some(Err(error)),
                None => break,
            };

            self.files = DwarfUnitFileIterator {
                unit: Some(unit),
                index: 0,
            };
        }

        self.finished = true;
        None
    }
}

/// An iterator over functions in a DWARF file.
pub struct DwarfFunctionIterator<'s> {
    units: DwarfUnitIterator<'s>,
    functions: std::vec::IntoIter<Function<'s>>,
    range_buf: Vec<Range>,
    finished: bool,
}

impl<'s> Iterator for DwarfFunctionIterator<'s> {
    type Item = Result<Function<'s>, DwarfError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        loop {
            if let Some(func) = self.functions.next() {
                return Some(Ok(func));
            }

            let unit = match self.units.next() {
                Some(Ok(unit)) => unit,
                Some(Err(error)) => return Some(Err(error)),
                None => break,
            };

            self.functions = match unit.functions(&mut self.range_buf) {
                Ok(functions) => functions.into_iter(),
                Err(error) => return Some(Err(error)),
            };
        }

        self.finished = true;
        None
    }
}

impl std::iter::FusedIterator for DwarfFunctionIterator<'_> {}
