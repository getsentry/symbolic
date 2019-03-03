//! Support for DWARF debugging information, common to ELF and MachO.
//!
//! The central element of this module is the [`Dwarf`] trait, which is implemented by [`ElfObject`]
//! and [`MachObject`]. The dwarf debug session object can be obtained via getters on those types.
//!
//! [`Dwarf`] trait.Dwarf.html
//! [`ElfObject`] ../elf/struct.ElfObject.html
//! [`MachObject`] ../macho/struct.MachObject.html

use std::borrow::Cow;
use std::ops::Deref;
use std::rc::Rc;

use failure::Fail;
use fallible_iterator::FallibleIterator;
use gimli::read::{AttributeValue, Range};
use gimli::{constants, UnitSectionOffset};
use lazycell::LazyCell;

use symbolic_common::{derive_failure, Language, Name};

use crate::base::*;

#[doc(hidden)]
pub use gimli;
pub use gimli::RunTimeEndian as Endian;

/// Helper that allows optional ownership of debug data in gimli.
#[derive(Clone, Debug, Default)]
struct RcCow<'a>(Rc<Cow<'a, [u8]>>);

impl<'a> RcCow<'a> {
    #[inline]
    fn new(cow: Cow<'a, [u8]>) -> Self {
        RcCow(Rc::new(cow))
    }
}

impl Deref for RcCow<'_> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

unsafe impl gimli::StableDeref for RcCow<'_> {}
unsafe impl gimli::CloneStableDeref for RcCow<'_> {}

impl<'a> From<Cow<'a, [u8]>> for RcCow<'a> {
    fn from(cow: Cow<'a, [u8]>) -> Self {
        Self::new(cow)
    }
}

impl<'a> From<&'a [u8]> for RcCow<'a> {
    fn from(slice: &'a [u8]) -> Self {
        Self::new(Cow::Borrowed(slice))
    }
}

impl<'a> From<Vec<u8>> for RcCow<'a> {
    fn from(vec: Vec<u8>) -> Self {
        Self::new(Cow::Owned(vec))
    }
}

trait ReadExt {
    fn to_string_lossy(&self) -> Cow<'_, str>;
}

impl ReadExt for gimli::read::EndianReader<Endian, RcCow<'_>> {
    #[inline]
    fn to_string_lossy(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self)
    }
}

type Slice<'a> = gimli::read::EndianReader<Endian, RcCow<'a>>;
type LocationLists<'a> = gimli::read::LocationLists<Slice<'a>>;
type RangeLists<'a> = gimli::read::RangeLists<Slice<'a>>;
type Unit<'a> = gimli::read::Unit<Slice<'a>>;
type DwarfInfo<'a> = gimli::read::Dwarf<Slice<'a>>;

type Die<'d, 'u> = gimli::read::DebuggingInformationEntry<'u, 'u, Slice<'d>, usize>;
type Attribute<'a> = gimli::read::Attribute<Slice<'a>>;
type UnitOffset = gimli::read::UnitOffset<usize>;
type DebugInfoOffset = gimli::DebugInfoOffset<usize>;

type CompilationUnitHeader<'a> = gimli::read::CompilationUnitHeader<Slice<'a>>;
type IncompleteLineNumberProgram<'a> = gimli::read::IncompleteLineProgram<Slice<'a>>;
type LineNumberProgramHeader<'a> = gimli::read::LineProgramHeader<Slice<'a>>;

/// Variants of [`DwarfError`](struct.DwarfError.html).
#[derive(Clone, Copy, Debug, Eq, Fail, PartialEq)]
pub enum DwarfErrorKind {
    /// A compilation unit referenced by index does not exist.
    #[fail(display = "compilation unit for offset {} does not exist", _0)]
    InvalidUnitRef(usize),

    /// A file record referenced by index does not exist.
    #[fail(display = "referenced file {} does not exist", _0)]
    InvalidFileRef(u64),

    /// An inline record was encountered without an inlining parent.
    #[fail(display = "unexpected inline function without parent")]
    UnexpectedInline,

    /// The debug_ranges of a function are invalid.
    #[fail(display = "function with inverted address range")]
    InvertedFunctionRange,

    /// The DWARF file is corrupted. See the cause for more information.
    #[fail(display = "corrupted dwarf debug data")]
    CorruptedData,
}

derive_failure!(
    DwarfError,
    DwarfErrorKind,
    doc = "An error handling [`DWARF`](trait.Dwarf.html) debugging information.",
);

impl From<gimli::read::Error> for DwarfError {
    fn from(error: gimli::read::Error) -> Self {
        error.context(DwarfErrorKind::CorruptedData).into()
    }
}

/// Provides access to DWARF debugging information independent of the container file type.
///
/// When implementing this trait, verify whether the container file type supports compressed section
/// data. If so, override the provided `section_data` method. Also, if there is a faster way to
/// check for the existence of a section without loading its data, override `has_section`.
pub trait Dwarf<'data> {
    /// Returns whether the file was written on a big-endian or little-endian machine.
    ///
    /// This can usually be determined by inspecting the file's headers. Sometimes, this is also
    /// given by the architecture.
    fn endianity(&self) -> Endian;

    /// Returns the offset and raw data of a section.
    ///
    /// The section name is given without leading punctuation, such dots or underscores. For
    /// instance, the name of the Debug Info section would be `"debug_info"`, which translates to
    /// `".debug_info"` in ELF and `"__debug_info"` in MachO.
    ///
    /// Certain containers might allow compressing section data. In this case, this function returns
    /// the compressed data. To get uncompressed data instead, use `section_data`.
    fn raw_data(&self, section: &str) -> Option<(u64, &'data [u8])>;

    /// Returns the offset and binary data of a section.
    ///
    /// If the section is compressed, this decompresses on the fly and returns allocated memory.
    /// Otherwise, this should return a slice of the raw data.
    ///
    /// The section name is given without leading punctuation, such dots or underscores. For
    /// instance, the name of the Debug Info section would be `"debug_info"`, which translates to
    /// `".debug_info"` in ELF and `"__debug_info"` in MachO.
    fn section_data(&self, section: &str) -> Option<(u64, Cow<'data, [u8]>)> {
        let (offset, data) = self.raw_data(section)?;
        Some((offset, Cow::Borrowed(data)))
    }

    /// Determines whether the specified section exists.
    ///
    /// The section name is given without leading punctuation, such dots or underscores. For
    /// instance, the name of the Debug Info section would be `"debug_info"`, which translates to
    /// `".debug_info"` in ELF and `"__debug_info"` in MachO.
    fn has_section(&self, section: &str) -> bool {
        self.raw_data(section).is_some()
    }
}

/// A row in the DWARF line program.
#[derive(Debug)]
struct DwarfRow {
    address: u64,
    file_index: u64,
    line: Option<u64>,
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

/// Wrapper around a DWARF Unit.
struct DwarfUnit<'d, 'a> {
    unit: &'a Unit<'d>,
    session: &'a DwarfDebugSession<'d>,
    language: Language,
    line_program: Option<DwarfLineProgram<'d>>,
}

impl<'d, 'a> DwarfUnit<'d, 'a> {
    /// Creates a DWARF unit from the gimli `Unit` type.
    fn from_unit(
        unit: &'a Unit<'d>,
        session: &'a DwarfDebugSession<'d>,
    ) -> Result<Self, DwarfError> {
        let mut entries = unit.entries();
        let entry = match entries.next_dfs()? {
            Some((_, entry)) => entry,
            None => return Err(gimli::read::Error::MissingUnitDie.into()),
        };

        let language = entry
            .attr(constants::DW_AT_language)?
            .and_then(|attr| match attr.value() {
                AttributeValue::Language(lang) => Some(language_from_dwarf(lang)),
                _ => None,
            })
            .unwrap_or(Language::Unknown);

        let line_program = match unit.line_program {
            Some(ref program) => Some(DwarfLineProgram::prepare(program.clone())?),
            None => None,
        };

        Ok(DwarfUnit {
            unit,
            session,
            language,
            line_program,
        })
    }

    /// Resolve the actual string value of an attribute.
    fn string_value(&self, value: AttributeValue<Slice<'d>>) -> Result<Cow<'d, str>, DwarfError> {
        // TODO(ja): Make this safe. The reader stored in `unit.comp_dir` holds on to a clone of the
        // Rc in the section storing the name of the compilation dir. At this point, we know that
        // this section will be held by the `DwarfDebuggingSession` instance, and all records
        // returned from this function borrow its lifetime.

        // It seems like there is no good solution to this other than cloning all debug sections at
        // the time `DwarfDebugSession` is created into an `gimli::EndianRcSlice` or alternatively
        // using a `SelfCell` to hold on to the buffer and the debug structs at the same time.
        let r = self.session.info.attr_string(self.unit, value)?;
        Ok(unsafe { std::mem::transmute(r.to_string_lossy()) })
    }

    /// The path of the compilation directory. File names are usually relative to this path.
    fn compilation_dir(&self) -> Cow<'d, str> {
        // TODO(ja): Make this safe. See the comments in `string_value`.
        match self.unit.comp_dir {
            Some(ref dir) => unsafe { std::mem::transmute(dir.to_string_lossy()) },
            None => Cow::default(),
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
                _ => match self.session.info.attr_ranges(self.unit, attr.value())? {
                    Some(mut ranges) => {
                        while let Some(range) = ranges.next()? {
                            range_buf.push(range);
                        }
                    }
                    None => continue,
                },
            }
        }

        // Found DW_AT_ranges, so early-exit here
        if !range_buf.is_empty() {
            return Ok(tuple);
        }

        // to go by the logic in dwarf2read a low_pc of 0 can indicate an
        // eliminated duplicate when the GNU linker is used.
        // TODO: *technically* there could be a relocatable section placed at VA 0
        let low_pc = match low_pc {
            Some(low_pc) if low_pc != 0 => low_pc,
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
            return Err(DwarfErrorKind::InvertedFunctionRange.into());
        }

        range_buf.push(Range {
            begin: low_pc,
            end: high_pc,
        });

        Ok(tuple)
    }

    /// Resolves an entry and if found invokes a function to transform it.
    ///
    /// As this might resolve into cached information the data borrowed from
    /// abbrev can only be temporarily accessed in the callback.
    fn resolve_reference<T, F>(&self, attr: Attribute<'d>, f: F) -> Result<Option<T>, DwarfError>
    where
        F: FnOnce(&Die<'d, '_>) -> Result<Option<T>, DwarfError>,
    {
        let (unit, offset) = match attr.value() {
            AttributeValue::UnitRef(offset) => (self.unit, offset),
            AttributeValue::DebugInfoRef(offset) => self.session.find_unit_offset(offset)?,
            // TODO: There is probably more that can come back here.
            _ => return Ok(None),
        };

        let mut entries = unit.entries_at_offset(offset)?;
        entries.next_entry()?;

        if let Some(entry) = entries.current() {
            f(entry)
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
                    return self.string_value(attr.value()).map(Some);
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
            return self.string_value(attr.value()).map(Some);
        }

        if let Some(attr) = reference_target {
            if let Some(name) =
                self.resolve_reference(attr, |ref_entry| self.resolve_function_name(ref_entry))?
            {
                return Ok(Some(name));
            }
        }

        Ok(None)
    }

    /// Resolves line records of a DIE's range list and puts them into the given buffer.
    fn resolve_lines(
        &self,
        ranges: &[Range],
        lines: &mut Vec<LineInfo<'d>>,
    ) -> Result<(), DwarfError> {
        // Early exit in case this unit did not declare a line program.
        let line_program = match self.line_program {
            Some(ref program) => program,
            None => return Ok(()),
        };

        let mut last = None;
        for range in ranges {
            for row in line_program.get_rows(range) {
                let file = self.resolve_file(row.file_index)?.unwrap();
                let line = row.line.unwrap_or(0);

                if let Some((last_file, last_line)) = last {
                    if last_file == row.file_index && last_line == line {
                        continue;
                    }
                }

                last = Some((row.file_index, line));
                lines.push(LineInfo {
                    address: row.address - self.session.load_address,
                    file,
                    line,
                });
            }
        }

        Ok(())
    }

    /// Resolves a file entry by its index.
    fn resolve_file(&self, file_id: u64) -> Result<Option<FileInfo<'d>>, DwarfError> {
        let line_program = match self.line_program {
            Some(ref program) => &program.header,
            None => return Ok(None),
        };

        let file = line_program
            .file(file_id)
            .ok_or_else(|| DwarfErrorKind::InvalidFileRef(file_id))?;

        Ok(Some(FileInfo {
            dir: file
                .directory(line_program)
                .and_then(|attr| self.string_value(attr).ok())
                .unwrap_or_default(),
            name: self.string_value(file.path_name()).unwrap_or_default(),
        }))
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

/// A stack for assembling function trees from lists of nested functions.
struct FunctionStack<'a>(Vec<(isize, Function<'a>)>);

impl<'a> FunctionStack<'a> {
    /// Creates a new function stack.
    pub fn new() -> Self {
        FunctionStack(Vec::with_capacity(16))
    }

    /// Pushes a new function onto the stack at the given depth.
    ///
    /// This assumes that `flush` has been called previously.
    pub fn push(&mut self, depth: isize, function: Function<'a>) {
        self.0.push((depth, function));
    }

    /// Peeks at the current top function (deepest inlining level).
    pub fn peek_mut(&mut self) -> Option<&mut Function<'a>> {
        self.0.last_mut().map(|&mut (_, ref mut function)| function)
    }

    /// Flushes all functions up to the given depth into the destination.
    ///
    /// This folds remaining functions into their parents. If a non-inlined function is encountered
    /// at or below the given depth, it is immediately flushed to the destination. Inlined functions
    /// are pushed into the inlinees list of their parents, instead.
    ///
    /// After this operation, the stack is either empty or its top function (see `peek`) will have a
    /// depth higher than the given depth. This allows to push new functions at this depth onto the
    /// stack.
    pub fn flush(&mut self, depth: isize, destination: &mut Vec<Function<'a>>) {
        let len = self.0.len();

        // Search for the first function that lies at or beyond the specified depth.
        let cutoff = self.0.iter().position(|&(d, _)| d >= depth).unwrap_or(len);

        // Pull functions from the stack. Inline functions are folded into their parents
        // transitively, while regular functions are returned. This also works when functions and
        // inlines are interleaved.
        let mut inlinee = None;
        for _ in cutoff..len {
            let (_, mut function) = self.0.pop().unwrap();
            if let Some(inlinee) = inlinee.take() {
                function.inlinees.push(inlinee);
            }

            if function.inline {
                inlinee = Some(function);
            } else {
                destination.push(function);
            }
        }

        // The top function in the flushed part of the stack was an inline function. Since it is
        // also being flushed out, we now append it to its parent. The topmost function in the stack
        // is verified to be a non-inline function before inserting.
        if let Some(inlinee) = inlinee {
            self.peek_mut().unwrap().inlinees.push(inlinee);
        }
    }
}

/// Loads and uncompresses section data and constructs a gimli record from it.
///
/// If the section is not present in the debug file, an empty record is created. If the section data
/// is compressed, it is uncompressed on the fly and moved into the record. Otherwise, the record is
/// created on a view onto the raw data.
fn load_gimli_section<'d, D, S>(dwarf: &D) -> S
where
    D: Dwarf<'d>,
    S: gimli::read::Section<Slice<'d>>,
{
    let data = dwarf
        .section_data(&S::section_name()[1..])
        .unwrap_or_default()
        .1;
    S::from(Slice::new(RcCow::new(data), dwarf.endianity()))
}

/// Constructs an empty gimli record without attempting to load the section data.
fn empty_gimli_section<'d, S>() -> S
where
    S: gimli::read::Section<Slice<'d>>,
{
    S::from(Slice::new(RcCow::default(), Default::default()))
}

/// A debugging session for DWARF debugging information.
pub struct DwarfDebugSession<'data> {
    info: DwarfInfo<'data>,
    headers: Vec<CompilationUnitHeader<'data>>,
    units: Vec<LazyCell<Option<Unit<'data>>>>,
    symbol_map: SymbolMap<'data>,
    load_address: u64,
}

impl<'d> DwarfDebugSession<'d> {
    /// Parses a dwarf debugging information from the given dwarf file.
    pub fn parse<D>(
        dwarf: &D,
        symbol_map: SymbolMap<'d>,
        load_address: u64,
    ) -> Result<Self, DwarfError>
    where
        D: Dwarf<'d>,
    {
        // Load required sections from the debug file. Unused sections are not loaded.
        let info = DwarfInfo {
            debug_abbrev: load_gimli_section(dwarf),
            debug_addr: empty_gimli_section(),
            debug_info: load_gimli_section(dwarf),
            debug_line: load_gimli_section(dwarf),
            debug_line_str: load_gimli_section(dwarf),
            debug_str: load_gimli_section(dwarf),
            debug_str_offsets: load_gimli_section(dwarf),
            debug_str_sup: empty_gimli_section(),
            debug_types: empty_gimli_section(),
            locations: LocationLists::new(empty_gimli_section(), empty_gimli_section()),
            ranges: RangeLists::new(load_gimli_section(dwarf), load_gimli_section(dwarf)),
        };

        // Prepare random access to unit headers.
        let headers = info.units().collect::<Vec<_>>()?;
        let units = headers.iter().map(|_| LazyCell::new()).collect();

        Ok(DwarfDebugSession {
            info,
            headers,
            units,
            symbol_map,
            load_address,
        })
    }

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
            let header = self.headers[index].clone();
            match self.info.unit(header) {
                Ok(unit) => Ok(Some(unit)),
                Err(gimli::read::Error::MissingUnitDie) => Ok(None),
                Err(error) => Err(DwarfError::from(error)),
            }
        })?;

        Ok(unit_opt.as_ref())
    }

    fn find_unit_offset(
        &self,
        offset: DebugInfoOffset,
    ) -> Result<(&Unit<'d>, UnitOffset), DwarfError> {
        let index = match self.headers.binary_search_by_key(&offset, |h| h.offset()) {
            Ok(index) => index,
            Err(0) => return Err(DwarfErrorKind::InvalidUnitRef(offset.0).into()),
            Err(next_index) => next_index - 1,
        };

        if let Some(unit) = self.get_unit(index)? {
            let offset = UnitSectionOffset::DebugInfoOffset(offset);
            if let Some(unit_offset) = offset.to_unit_offset(unit) {
                return Ok((unit, unit_offset));
            }
        }

        Err(DwarfErrorKind::InvalidUnitRef(offset.0).into())
    }
}

impl<'d> DebugSession for DwarfDebugSession<'d> {
    type Error = DwarfError;

    fn functions(&mut self) -> Result<Vec<Function<'_>>, Self::Error> {
        let mut range_buf = Vec::new();
        let mut line_buf = Vec::new();
        let mut functions = Vec::new();

        for index in 0..self.headers.len() {
            let unit = match self.get_unit(index)? {
                Some(unit) => DwarfUnit::from_unit(unit, self)?,
                None => continue,
            };

            let mut depth = 0;
            let mut skipped_depth = None;
            let batch_start = functions.len();

            let mut stack = FunctionStack::new();
            let mut entries = unit.unit.entries();
            while let Some((movement, entry)) = entries.next_dfs()? {
                depth += movement;

                // If we're navigating within a skipped function (see below), we can ignore this
                // entry completely. Otherwise, we've moved out of any skipped function and can
                // reset the stored depth.
                match skipped_depth {
                    Some(skipped) if depth > skipped => continue,
                    _ => skipped_depth = None,
                }

                // Skip anything that is not a function.
                let inline = match entry.tag() {
                    constants::DW_TAG_subprogram => false,
                    constants::DW_TAG_inlined_subroutine => true,
                    _ => continue,
                };

                // Flush all functions out that exceed the current iteration depth. Since we
                // encountered a function at this level, there will be no more inlinees to the
                // previous function at the same level or any of it's children.
                stack.flush(depth, &mut functions);

                range_buf.clear();
                let (call_line, call_file) = unit.parse_ranges(entry, &mut range_buf)?;

                // Ranges can be empty for two reasons: (1) the function is a no-op and does not
                // contain any code, or (2) the function did contain eliminated dead code. In the
                // latter case, a surrogate DIE remains with `DW_AT_low_pc(0)` and empty ranges.
                // That DIE might still contain inlined functions with actual ranges, which must all
                // be skipped.
                if range_buf.is_empty() {
                    skipped_depth = Some(depth);
                    continue;
                }

                let function_address = range_buf[0].begin - self.load_address;
                let function_size = range_buf[range_buf.len() - 1].end - range_buf[0].begin;

                // Resolve functions in the symbol table first. Only if there is no entry, fall back
                // to debug information only if there is no match. Sometimes, debug info contains a
                // lesser quality of symbol names.
                //
                // XXX: Maybe we should actually parse the ranges in the resolve function and always
                // look at the symbol table based on the start of the DIE range.
                let symbol_name = if !inline {
                    self.symbol_map
                        .lookup_range(function_address..function_address + function_size)
                        .and_then(|symbol| symbol.name.clone())
                } else {
                    None
                };

                let name = match symbol_name {
                    Some(name) => Some(name),
                    None => unit.resolve_function_name(entry)?,
                };

                // Avoid constant allocations by collecting repeatedly into the same buffer and
                // draining the results out of it. This keeps the original buffer allocated and
                // allows for a single allocation per call to `resolve_lines`.
                unit.resolve_lines(&range_buf, &mut line_buf)?;

                let function = Function {
                    address: function_address,
                    size: function_size,
                    name: Name::with_language(name.unwrap_or_default(), unit.language),
                    compilation_dir: unit.compilation_dir(),
                    lines: line_buf.drain(..).collect(),
                    inlinees: Vec::new(),
                    inline,
                };

                if inline {
                    // An inlined function must always have a parent. An empty list of funcs
                    // indicates invalid debug information.
                    let parent = match stack.peek_mut() {
                        Some(parent) => parent,
                        None => return Err(DwarfErrorKind::UnexpectedInline.into()),
                    };

                    // Make sure there is correct line information for the call site of this inlined
                    // function. In general, a compiler should always output the call line and call
                    // file for inlined subprograms. If this info is missing, the lookup might
                    // return invalid line numbers.
                    if let (Some(line), Some(file_id)) = (call_line, call_file) {
                        if let Some(file) = unit.resolve_file(file_id)? {
                            match parent
                                .lines
                                .binary_search_by_key(&function_address, |line| line.address)
                            {
                                Ok(idx) => {
                                    // We found a line record that points to this function. This happens
                                    // especially, if the function range overlaps exactly. Patch the
                                    // call info with the correct location.
                                    parent.lines[idx].file = file;
                                    parent.lines[idx].line = line;
                                }
                                Err(idx) => {
                                    // There is no line record pointing to this function, so add one to
                                    // the correct call location. Note that "base_dir" can be inherited
                                    // safely here.
                                    let line_info = LineInfo {
                                        address: function_address,
                                        file,
                                        line,
                                    };
                                    parent.lines.insert(idx, line_info);
                                }
                            }
                        }
                    }
                }

                stack.push(depth, function)
            }

            // We're done, flush the remaining stack.
            stack.flush(0, &mut functions);

            // Units are sorted by their address range in DWARF, but the functions within may occurr
            // in any order. Sort the batch that was just written, therefore.
            dmsort::sort_by_key(&mut functions[batch_start..], |f| f.address);
        }

        Ok(functions)
    }
}
