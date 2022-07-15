//! Support for DWARF debugging information, common to ELF and MachO.
//!
//! The central element of this module is the [`Dwarf`] trait, which is implemented by [`ElfObject`]
//! and [`MachObject`]. The dwarf debug session object can be obtained via getters on those types.
//!
//! [`Dwarf`]: trait.Dwarf.html
//! [`ElfObject`]: ../elf/struct.ElfObject.html
//! [`MachObject`]: ../macho/struct.MachObject.html

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::Arc;

use fallible_iterator::FallibleIterator;
use gimli::read::{AttributeValue, Error as GimliError, Range};
use gimli::{constants, DwarfFileType, UnitSectionOffset};
use lazycell::LazyCell;
use thiserror::Error;

use symbolic_common::{AsSelf, Language, Name, NameMangling, SelfCell};

use crate::base::*;
use crate::function_builder::FunctionBuilder;
#[cfg(feature = "macho")]
use crate::macho::BcSymbolMap;

/// This is a fake BcSymbolMap used when macho support is turned off since they are unfortunately
/// part of the dwarf interface
#[cfg(not(feature = "macho"))]
#[derive(Debug)]
pub struct BcSymbolMap<'d> {
    _marker: std::marker::PhantomData<&'d str>,
}

#[cfg(not(feature = "macho"))]
impl<'d> BcSymbolMap<'d> {
    pub(crate) fn resolve_opt(&self, _name: impl AsRef<[u8]>) -> Option<&str> {
        None
    }
}

#[doc(hidden)]
pub use gimli;
pub use gimli::RunTimeEndian as Endian;

type Slice<'a> = gimli::read::EndianSlice<'a, Endian>;
type RangeLists<'a> = gimli::read::RangeLists<Slice<'a>>;
type Unit<'a> = gimli::read::Unit<Slice<'a>>;
type DwarfInner<'a> = gimli::read::Dwarf<Slice<'a>>;

type Die<'d, 'u> = gimli::read::DebuggingInformationEntry<'u, 'u, Slice<'d>, usize>;
type Attribute<'a> = gimli::read::Attribute<Slice<'a>>;
type UnitOffset = gimli::read::UnitOffset<usize>;
type DebugInfoOffset = gimli::DebugInfoOffset<usize>;
type EntriesRaw<'d, 'u> = gimli::EntriesRaw<'u, 'u, Slice<'d>>;

type UnitHeader<'a> = gimli::read::UnitHeader<Slice<'a>>;
type IncompleteLineNumberProgram<'a> = gimli::read::IncompleteLineProgram<Slice<'a>>;
type LineNumberProgramHeader<'a> = gimli::read::LineProgramHeader<Slice<'a>>;
type LineProgramFileEntry<'a> = gimli::read::FileEntry<Slice<'a>>;

/// This applies the offset to the address.
///
/// This function does not panic but would wrap around if too large or small
/// numbers are passed.
fn offset(addr: u64, offset: i64) -> u64 {
    (addr as i64).wrapping_sub(offset as i64) as u64
}

/// The error type for [`DwarfError`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DwarfErrorKind {
    /// A compilation unit referenced by index does not exist.
    InvalidUnitRef(usize),

    /// A file record referenced by index does not exist.
    InvalidFileRef(u64),

    /// An inline record was encountered without an inlining parent.
    UnexpectedInline,

    /// The debug_ranges of a function are invalid.
    InvertedFunctionRange,

    /// The DWARF file is corrupted. See the cause for more information.
    CorruptedData,
}

impl fmt::Display for DwarfErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUnitRef(offset) => {
                write!(f, "compilation unit for offset {} does not exist", offset)
            }
            Self::InvalidFileRef(id) => write!(f, "referenced file {} does not exist", id),
            Self::UnexpectedInline => write!(f, "unexpected inline function without parent"),
            Self::InvertedFunctionRange => write!(f, "function with inverted address range"),
            Self::CorruptedData => write!(f, "corrupted dwarf debug data"),
        }
    }
}

/// An error handling [`DWARF`](trait.Dwarf.html) debugging information.
#[derive(Debug, Error)]
#[error("{kind}")]
pub struct DwarfError {
    kind: DwarfErrorKind,
    #[source]
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl DwarfError {
    /// Creates a new DWARF error from a known kind of error as well as an arbitrary error
    /// payload.
    fn new<E>(kind: DwarfErrorKind, source: E) -> Self
    where
        E: Into<Box<dyn Error + Send + Sync>>,
    {
        let source = Some(source.into());
        Self { kind, source }
    }

    /// Returns the corresponding [`DwarfErrorKind`] for this error.
    pub fn kind(&self) -> DwarfErrorKind {
        self.kind
    }
}

impl From<DwarfErrorKind> for DwarfError {
    fn from(kind: DwarfErrorKind) -> Self {
        Self { kind, source: None }
    }
}

impl From<GimliError> for DwarfError {
    fn from(e: GimliError) -> Self {
        Self::new(DwarfErrorKind::CorruptedData, e)
    }
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

impl<'d> DwarfLineProgram<'d> {
    fn prepare(program: IncompleteLineNumberProgram<'d>) -> Self {
        let mut sequences = Vec::new();
        let mut sequence_rows = Vec::<DwarfRow>::new();
        let mut prev_address = 0;
        let mut state_machine = program.rows();

        while let Ok(Some((_, &program_row))) = state_machine.next_row() {
            let address = program_row.address();

            // we have seen rustc emit for WASM targets a bad sequence that spans from 0 to
            // the end of the program.  https://github.com/rust-lang/rust/issues/79410
            // Since DWARF does not permit code to sit at address 0 we can safely skip here.
            if address == 0 {
                continue;
            }

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
                let line = program_row.line().map(|v| v.get());
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

        DwarfLineProgram {
            header: state_machine.header().clone(),
            sequences,
        }
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
        F: FnOnce(Self, &Die<'d, '_>) -> Result<Option<T>, DwarfError>,
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

    /// Returns the offset of this unit within its section.
    fn offset(&self) -> UnitSectionOffset {
        self.unit.header.offset()
    }

    /// Resolves the function name of a debug entry.
    fn resolve_function_name(
        &self,
        entry: &Die<'d, '_>,
        language: Language,
        bcsymbolmap: Option<&'d BcSymbolMap<'d>>,
    ) -> Result<Option<Name<'d>>, DwarfError> {
        let mut attrs = entry.attrs();
        let mut fallback_name = None;
        let mut reference_target = None;

        while let Some(attr) = attrs.next()? {
            match attr.name() {
                // Prioritize these. If we get them, take them.
                constants::DW_AT_linkage_name | constants::DW_AT_MIPS_linkage_name => {
                    return Ok(self
                        .string_value(attr.value())
                        .map(|n| resolve_cow_name(bcsymbolmap, n))
                        .map(|n| Name::new(n, NameMangling::Mangled, language)));
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
            return Ok(self
                .string_value(attr.value())
                .map(|n| resolve_cow_name(bcsymbolmap, n))
                .map(|n| Name::new(n, NameMangling::Unmangled, language)));
        }

        if let Some(attr) = reference_target {
            return self.resolve_reference(attr, |ref_unit, ref_entry| {
                if self.offset() != ref_unit.offset() || entry.offset() != ref_entry.offset() {
                    ref_unit.resolve_function_name(ref_entry, language, bcsymbolmap)
                } else {
                    Ok(None)
                }
            });
        }

        Ok(None)
    }
}

/// Wrapper around a DWARF Unit.
#[derive(Debug)]
struct DwarfUnit<'d, 'a> {
    inner: UnitRef<'d, 'a>,
    bcsymbolmap: Option<&'d BcSymbolMap<'d>>,
    language: Language,
    line_program: Option<DwarfLineProgram<'d>>,
    prefer_dwarf_names: bool,
}

impl<'d, 'a> DwarfUnit<'d, 'a> {
    /// Creates a DWARF unit from the gimli `Unit` type.
    fn from_unit(
        unit: &'a Unit<'d>,
        info: &'a DwarfInfo<'d>,
        bcsymbolmap: Option<&'d BcSymbolMap<'d>>,
    ) -> Result<Option<Self>, DwarfError> {
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

        let line_program = unit
            .line_program
            .as_ref()
            .map(|program| DwarfLineProgram::prepare(program.clone()));

        let producer = match entry.attr_value(constants::DW_AT_producer)? {
            Some(AttributeValue::String(string)) => Some(string),
            _ => None,
        };

        // Trust the symbol table more to contain accurate mangled names. However, since Dart's name
        // mangling is lossy, we need to load the demangled name instead.
        let prefer_dwarf_names = producer.as_deref() == Some(b"Dart VM");

        Ok(Some(DwarfUnit {
            inner: UnitRef { info, unit },
            bcsymbolmap,
            language,
            line_program,
            prefer_dwarf_names,
        }))
    }

    /// The path of the compilation directory. File names are usually relative to this path.
    fn compilation_dir(&self) -> &'d [u8] {
        match self.inner.unit.comp_dir {
            Some(ref dir) => resolve_byte_name(self.bcsymbolmap, dir.slice()),
            None => &[],
        }
    }

    /// Parses the call site and range lists of this Debugging Information Entry.
    ///
    /// This method consumes the attributes of the DIE. This means that the `entries` iterator must
    /// be placed just before the attributes of the DIE. On return, the `entries` iterator is placed
    /// after the attributes, ready to read the next DIE's abbrev.
    fn parse_ranges<'r>(
        &self,
        entries: &mut EntriesRaw<'d, '_>,
        abbrev: &gimli::Abbreviation,
        range_buf: &'r mut Vec<Range>,
    ) -> Result<(&'r mut Vec<Range>, CallLocation), DwarfError> {
        range_buf.clear();

        let mut call_line = None;
        let mut call_file = None;
        let mut low_pc = None;
        let mut high_pc = None;
        let mut high_pc_rel = None;

        let kind = self.inner.info.kind;

        for spec in abbrev.attributes() {
            let attr = entries.read_attribute(*spec)?;
            match attr.name() {
                constants::DW_AT_low_pc => match attr.value() {
                    AttributeValue::Addr(addr) => low_pc = Some(addr),
                    AttributeValue::DebugAddrIndex(index) => {
                        low_pc = Some(self.inner.info.address(self.inner.unit, index)?)
                    }
                    _ => return Err(GimliError::UnsupportedAttributeForm.into()),
                },
                constants::DW_AT_high_pc => match attr.value() {
                    AttributeValue::Addr(addr) => high_pc = Some(addr),
                    AttributeValue::DebugAddrIndex(index) => {
                        high_pc = Some(self.inner.info.address(self.inner.unit, index)?)
                    }
                    AttributeValue::Udata(size) => high_pc_rel = Some(size),
                    _ => return Err(GimliError::UnsupportedAttributeForm.into()),
                },
                constants::DW_AT_call_line => match attr.value() {
                    AttributeValue::Udata(line) => call_line = Some(line),
                    _ => return Err(GimliError::UnsupportedAttributeForm.into()),
                },
                constants::DW_AT_call_file => match attr.value() {
                    AttributeValue::FileIndex(file) => call_file = Some(file),
                    _ => return Err(GimliError::UnsupportedAttributeForm.into()),
                },
                constants::DW_AT_ranges
                | constants::DW_AT_rnglists_base
                | constants::DW_AT_start_scope => {
                    match self.inner.info.attr_ranges(self.inner.unit, attr.value())? {
                        Some(mut ranges) => {
                            while let Some(range) = match ranges.next() {
                                Ok(range) => range,
                                // We have seen broken ranges for some WASM debug files generated by
                                // emscripten. They mostly manifest themselves in these errors, which
                                // are triggered by an inverted range (going high to low).
                                // See a few more examples of broken ranges here:
                                // https://github.com/emscripten-core/emscripten/issues/15552
                                Err(gimli::Error::InvalidAddressRange) => None,
                                Err(err) => {
                                    return Err(err.into());
                                }
                            } {
                                // A range that begins at 0 indicates code that was eliminated by
                                // the linker, see below.
                                if range.begin > 0 || kind == ObjectKind::Relocatable {
                                    range_buf.push(range);
                                }
                            }
                        }
                        None => continue,
                    }
                }
                _ => continue,
            }
        }

        let call_location = CallLocation {
            call_file,
            call_line,
        };

        if range_buf.is_empty() {
            if let Some(range) = Self::convert_pc_range(low_pc, high_pc, high_pc_rel, kind)? {
                range_buf.push(range);
            }
        }

        Ok((range_buf, call_location))
    }

    fn convert_pc_range(
        low_pc: Option<u64>,
        high_pc: Option<u64>,
        high_pc_rel: Option<u64>,
        kind: ObjectKind,
    ) -> Result<Option<Range>, DwarfError> {
        // To go by the logic in dwarf2read, a `low_pc` of 0 can indicate an
        // eliminated duplicate when the GNU linker is used. In relocatable
        // objects, all functions are at `0` since they have not been placed
        // yet, so we want to retain them.
        let low_pc = match low_pc {
            Some(low_pc) if low_pc != 0 || kind == ObjectKind::Relocatable => low_pc,
            _ => return Ok(None),
        };

        let high_pc = match (high_pc, high_pc_rel) {
            (Some(high_pc), _) => high_pc,
            (_, Some(high_pc_rel)) => low_pc.wrapping_add(high_pc_rel),
            _ => return Ok(None),
        };

        if low_pc == high_pc {
            // Most likely low_pc == high_pc means the DIE should be ignored.
            // https://sourceware.org/ml/gdb-patches/2011-03/msg00739.html
            return Ok(None);
        }

        if low_pc == u64::MAX || low_pc == u64::MAX - 1 {
            // Similarly, u64::MAX/u64::MAX-1 may be used to indicate deleted code.
            // See https://reviews.llvm.org/D59553
            return Ok(None);
        }

        if low_pc > high_pc {
            return Err(DwarfErrorKind::InvertedFunctionRange.into());
        }

        Ok(Some(Range {
            begin: low_pc,
            end: high_pc,
        }))
    }

    /// Resolves file information from a line program.
    fn file_info(
        &self,
        line_program: &LineNumberProgramHeader<'d>,
        file: &LineProgramFileEntry<'d>,
    ) -> FileInfo<'d> {
        FileInfo {
            dir: resolve_byte_name(
                self.bcsymbolmap,
                file.directory(line_program)
                    .and_then(|attr| self.inner.slice_value(attr))
                    .unwrap_or_default(),
            ),
            name: resolve_byte_name(
                self.bcsymbolmap,
                self.inner.slice_value(file.path_name()).unwrap_or_default(),
            ),
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

    /// Resolves the name of a function from the symbol table.
    fn resolve_symbol_name(&self, address: u64) -> Option<Name<'d>> {
        let symbol = self.inner.info.symbol_map.lookup_exact(address)?;
        let name = resolve_cow_name(self.bcsymbolmap, symbol.name.clone()?);
        Some(Name::new(name, NameMangling::Mangled, self.language))
    }

    /// Resolves the name of a function from DWARF debug information.
    fn resolve_dwarf_name(&self, entry: &Die<'d, '_>) -> Option<Name<'d>> {
        self.inner
            .resolve_function_name(entry, self.language, self.bcsymbolmap)
            .ok()
            .flatten()
    }

    /// Parses any DW_TAG_subprogram DIEs in the DIE subtree.
    fn parse_functions(
        &self,
        depth: isize,
        entries: &mut EntriesRaw<'d, '_>,
        output: &mut FunctionsOutput<'_, 'd>,
    ) -> Result<(), DwarfError> {
        while !entries.is_empty() {
            let dw_die_offset = entries.next_offset();
            let next_depth = entries.next_depth();
            if next_depth <= depth {
                return Ok(());
            }
            if let Some(abbrev) = entries.read_abbreviation()? {
                if abbrev.tag() == constants::DW_TAG_subprogram {
                    self.parse_function(dw_die_offset, next_depth, entries, abbrev, output)?;
                } else {
                    entries.skip_attributes(abbrev.attributes())?;
                }
            }
        }
        Ok(())
    }

    /// Parse a single function from a DWARF DIE subtree.
    ///
    /// The `entries` iterator must be placed after the abbrev / before the attributes of the
    /// function DIE.
    ///
    /// This method can call itself recursively if another DW_TAG_subprogram entry is encountered
    /// in the subtree.
    ///
    /// On return, the `entries` iterator is placed after the attributes of the last-read DIE.
    fn parse_function(
        &self,
        dw_die_offset: gimli::UnitOffset<usize>,
        depth: isize,
        entries: &mut EntriesRaw<'d, '_>,
        abbrev: &gimli::Abbreviation,
        output: &mut FunctionsOutput<'_, 'd>,
    ) -> Result<(), DwarfError> {
        let (ranges, _) = self.parse_ranges(entries, abbrev, &mut output.range_buf)?;

        let seen_ranges = &mut *output.seen_ranges;
        ranges.retain(|range| {
            // Filter out empty and reversed ranges.
            if range.begin > range.end {
                return false;
            }

            // We have seen duplicate top-level function entries being yielded from the
            // [`DwarfFunctionIterator`], which combined with recursively walking its inlinees can
            // blow past symcache limits.
            // We suspect the reason is that the the same top-level functions might be defined in
            // different compile units. We suspect this might be caused by link-time deduplication
            // which merges templated code that is being generated multiple times in each
            // compilation unit. We make sure to detect this here, so we can avoid creating these
            // duplicates as early as possible.
            let address = offset(range.begin, self.inner.info.address_offset);
            let size = range.end - range.begin;

            seen_ranges.insert((address, size))
        });

        // Ranges can be empty for three reasons: (1) the function is a no-op and does not
        // contain any code, (2) the function did contain eliminated dead code, or (3) some
        // tooling created garbage reversed ranges which we filtered out.
        // In the dead code case, a surrogate DIE remains with `DW_AT_low_pc(0)` and empty ranges.
        // That DIE might still contain inlined functions with actual ranges - these must be skipped.
        // However, non-inlined functions may be present in this subtree, so we must still descend
        // into it.
        if ranges.is_empty() {
            return self.parse_functions(depth, entries, output);
        }

        // Resolve functions in the symbol table first. Only if there is no entry, fall back
        // to debug information only if there is no match. Sometimes, debug info contains a
        // lesser quality of symbol names.
        //
        // XXX: Maybe we should actually parse the ranges in the resolve function and always
        // look at the symbol table based on the start of the DIE range.
        let symbol_name = if self.prefer_dwarf_names {
            None
        } else {
            let first_range_begin = ranges.iter().map(|range| range.begin).min().unwrap();
            let function_address = offset(first_range_begin, self.inner.info.address_offset);
            self.resolve_symbol_name(function_address)
        };

        let name = symbol_name
            .or_else(|| self.resolve_dwarf_name(&self.inner.unit.entry(dw_die_offset).unwrap()))
            .unwrap_or_else(|| Name::new("", NameMangling::Unmangled, self.language));

        // Create one function per range. In the common case there is only one range, so
        // we usually only have one function builder here.
        let mut builders: Vec<(Range, FunctionBuilder)> = ranges
            .iter()
            .map(|range| {
                let address = offset(range.begin, self.inner.info.address_offset);
                let size = range.end - range.begin;
                (
                    *range,
                    FunctionBuilder::new(name.clone(), self.compilation_dir(), address, size),
                )
            })
            .collect();

        self.parse_function_children(depth, 0, entries, &mut builders, output)?;

        if let Some(line_program) = &self.line_program {
            for (range, builder) in &mut builders {
                for row in line_program.get_rows(range) {
                    let address = offset(row.address, self.inner.info.address_offset);
                    let size = row.size;
                    let file = self.resolve_file(row.file_index).unwrap_or_default();
                    let line = row.line.unwrap_or(0);
                    builder.add_leaf_line(address, size, file, line);
                }
            }
        }

        for (_range, builder) in builders {
            output.functions.push(builder.finish());
        }

        Ok(())
    }

    /// Traverses a subtree during function parsing.
    fn parse_function_children(
        &self,
        depth: isize,
        inline_depth: u32,
        entries: &mut EntriesRaw<'d, '_>,
        builders: &mut [(Range, FunctionBuilder<'d>)],
        output: &mut FunctionsOutput<'_, 'd>,
    ) -> Result<(), DwarfError> {
        while !entries.is_empty() {
            let dw_die_offset = entries.next_offset();
            let next_depth = entries.next_depth();
            if next_depth <= depth {
                return Ok(());
            }
            let abbrev = match entries.read_abbreviation()? {
                Some(abbrev) => abbrev,
                None => continue,
            };
            match abbrev.tag() {
                constants::DW_TAG_subprogram => {
                    self.parse_function(dw_die_offset, next_depth, entries, abbrev, output)?;
                }
                constants::DW_TAG_inlined_subroutine => {
                    self.parse_inlinee(
                        dw_die_offset,
                        next_depth,
                        inline_depth + 1,
                        entries,
                        abbrev,
                        builders,
                        output,
                    )?;
                }
                _ => {
                    entries.skip_attributes(abbrev.attributes())?;
                }
            }
        }
        Ok(())
    }

    /// Recursively parse the inlinees of a function from a DWARF DIE subtree.
    ///
    /// The `entries` iterator must be placed just before the attributes of the inline function DIE.
    ///
    /// This method calls itself recursively for other DW_TAG_inlined_subroutine entries in the
    /// subtree. It can also call `parse_function` if a `DW_TAG_subprogram` entry is encountered.
    ///
    /// On return, the `entries` iterator is placed after the attributes of the last-read DIE.
    #[allow(clippy::too_many_arguments)]
    fn parse_inlinee(
        &self,
        dw_die_offset: gimli::UnitOffset<usize>,
        depth: isize,
        inline_depth: u32,
        entries: &mut EntriesRaw<'d, '_>,
        abbrev: &gimli::Abbreviation,
        builders: &mut [(Range, FunctionBuilder<'d>)],
        output: &mut FunctionsOutput<'_, 'd>,
    ) -> Result<(), DwarfError> {
        let (ranges, call_location) = self.parse_ranges(entries, abbrev, &mut output.range_buf)?;

        ranges.retain(|range| range.end > range.begin);

        // Ranges can be empty for three reasons: (1) the function is a no-op and does not
        // contain any code, (2) the function did contain eliminated dead code, or (3) some
        // tooling created garbage reversed ranges which we filtered out.
        // In the dead code case, a surrogate DIE remains with `DW_AT_low_pc(0)` and empty ranges.
        // That DIE might still contain inlined functions with actual ranges - these must be skipped.
        // However, non-inlined functions may be present in this subtree, so we must still descend
        // into it.
        if ranges.is_empty() {
            return self.parse_functions(depth, entries, output);
        }

        let name = self
            .resolve_dwarf_name(&self.inner.unit.entry(dw_die_offset).unwrap())
            .unwrap_or_else(|| Name::new("", NameMangling::Unmangled, self.language));

        let call_file = call_location
            .call_file
            .and_then(|i| self.resolve_file(i))
            .unwrap_or_default();
        let call_line = call_location.call_line.unwrap_or(0);

        // Create a separate inlinee for each range.
        for range in ranges.iter() {
            // Find the builder for the outer function that covers this range. Usually there's only
            // one outer range, so only one builder.
            let builder = match builders.iter_mut().find(|(outer_range, _builder)| {
                range.begin >= outer_range.begin && range.begin < outer_range.end
            }) {
                Some((_outer_range, builder)) => builder,
                None => continue,
            };

            let address = offset(range.begin, self.inner.info.address_offset);
            let size = range.end - range.begin;
            builder.add_inlinee(
                inline_depth,
                name.clone(),
                address,
                size,
                call_file.clone(),
                call_line,
            );
        }

        self.parse_function_children(depth, inline_depth, entries, builders, output)
    }

    /// Collects all functions within this compilation unit.
    fn functions(
        &self,
        seen_ranges: &mut BTreeSet<(u64, u64)>,
    ) -> Result<Vec<Function<'d>>, DwarfError> {
        let mut entries = self.inner.unit.entries_raw(None)?;
        let mut output = FunctionsOutput::with_seen_ranges(seen_ranges);
        self.parse_functions(-1, &mut entries, &mut output)?;
        Ok(output.functions)
    }
}

/// The state we pass around during function parsing.
struct FunctionsOutput<'a, 'd> {
    /// The list of fully-parsed outer functions. Items are appended whenever we are done
    /// parsing an entire function.
    pub functions: Vec<Function<'d>>,
    /// A scratch buffer which avoids frequent allocations.
    pub range_buf: Vec<Range>,
    /// The set of `(address, size)` ranges of the functions we've already parsed.
    pub seen_ranges: &'a mut BTreeSet<(u64, u64)>,
}

impl<'a, 'd> FunctionsOutput<'a, 'd> {
    pub fn with_seen_ranges(seen_ranges: &'a mut BTreeSet<(u64, u64)>) -> Self {
        Self {
            functions: Vec::new(),
            range_buf: Vec::new(),
            seen_ranges,
        }
    }
}

/// For returning (partial) call location information from `parse_ranges`.
#[derive(Debug, Default, Clone, Copy)]
struct CallLocation {
    pub call_file: Option<u64>,
    pub call_line: Option<u64>,
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
    fn from_dwarf<D>(dwarf: &D) -> Self
    where
        D: Dwarf<'data>,
    {
        DwarfSections {
            debug_abbrev: DwarfSectionData::load(dwarf),
            debug_info: DwarfSectionData::load(dwarf),
            debug_line: DwarfSectionData::load(dwarf),
            debug_line_str: DwarfSectionData::load(dwarf),
            debug_str: DwarfSectionData::load(dwarf),
            debug_str_offsets: DwarfSectionData::load(dwarf),
            debug_ranges: DwarfSectionData::load(dwarf),
            debug_rnglists: DwarfSectionData::load(dwarf),
        }
    }
}

struct DwarfInfo<'data> {
    inner: DwarfInner<'data>,
    headers: Vec<UnitHeader<'data>>,
    units: Vec<LazyCell<Option<Unit<'data>>>>,
    symbol_map: SymbolMap<'data>,
    address_offset: i64,
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
        address_offset: i64,
        kind: ObjectKind,
    ) -> Result<Self, DwarfError> {
        let inner = gimli::read::Dwarf {
            debug_abbrev: sections.debug_abbrev.to_gimli(),
            debug_addr: Default::default(),
            debug_aranges: Default::default(),
            debug_info: sections.debug_info.to_gimli(),
            debug_line: sections.debug_line.to_gimli(),
            debug_line_str: sections.debug_line_str.to_gimli(),
            debug_str: sections.debug_str.to_gimli(),
            debug_str_offsets: sections.debug_str_offsets.to_gimli(),
            debug_types: Default::default(),
            locations: Default::default(),
            ranges: RangeLists::new(
                sections.debug_ranges.to_gimli(),
                sections.debug_rnglists.to_gimli(),
            ),
            file_type: DwarfFileType::Main,
            sup: Default::default(),
        };

        // Prepare random access to unit headers.
        let headers = inner.units().collect::<Vec<_>>()?;
        let units = headers.iter().map(|_| LazyCell::new()).collect();

        Ok(DwarfInfo {
            inner,
            headers,
            units,
            symbol_map,
            address_offset,
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
        let section_offset = UnitSectionOffset::DebugInfoOffset(offset);
        let search_result = self
            .headers
            .binary_search_by_key(&section_offset, UnitHeader::offset);

        let index = match search_result {
            Ok(index) => index,
            Err(0) => return Err(DwarfErrorKind::InvalidUnitRef(offset.0).into()),
            Err(next_index) => next_index - 1,
        };

        if let Some(unit) = self.get_unit(index)? {
            if let Some(unit_offset) = section_offset.to_unit_offset(unit) {
                return Ok((UnitRef { unit, info: self }, unit_offset));
            }
        }

        Err(DwarfErrorKind::InvalidUnitRef(offset.0).into())
    }

    /// Returns an iterator over all compilation units.
    fn units(&'d self, bcsymbolmap: Option<&'d BcSymbolMap<'d>>) -> DwarfUnitIterator<'_> {
        DwarfUnitIterator {
            info: self,
            bcsymbolmap,
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
            .field("address_offset", &self.address_offset)
            .finish()
    }
}

/// An iterator over compilation units in a DWARF object.
struct DwarfUnitIterator<'s> {
    info: &'s DwarfInfo<'s>,
    bcsymbolmap: Option<&'s BcSymbolMap<'s>>,
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

            match DwarfUnit::from_unit(unit, self.info, self.bcsymbolmap) {
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
    bcsymbolmap: Option<Arc<BcSymbolMap<'data>>>,
}

impl<'data> DwarfDebugSession<'data> {
    /// Parses a dwarf debugging information from the given DWARF file.
    pub fn parse<D>(
        dwarf: &D,
        symbol_map: SymbolMap<'data>,
        address_offset: i64,
        kind: ObjectKind,
    ) -> Result<Self, DwarfError>
    where
        D: Dwarf<'data>,
    {
        let sections = DwarfSections::from_dwarf(dwarf);
        let cell = SelfCell::try_new(Box::new(sections), |sections| {
            DwarfInfo::parse(unsafe { &*sections }, symbol_map, address_offset, kind)
        })?;

        Ok(DwarfDebugSession {
            cell,
            bcsymbolmap: None,
        })
    }

    /// Loads the [`BcSymbolMap`] into this debug session.
    ///
    /// All the file and function names yielded by this debug session will be resolved using
    /// the provided symbol map.
    #[cfg(feature = "macho")]
    pub(crate) fn load_symbolmap(&mut self, symbolmap: Option<Arc<BcSymbolMap<'data>>>) {
        self.bcsymbolmap = symbolmap;
    }

    /// Returns an iterator over all source files in this debug file.
    pub fn files(&self) -> DwarfFileIterator<'_> {
        DwarfFileIterator {
            units: self.cell.get().units(self.bcsymbolmap.as_deref()),
            files: DwarfUnitFileIterator::default(),
            finished: false,
        }
    }

    /// Returns an iterator over all functions in this debug file.
    pub fn functions(&self) -> DwarfFunctionIterator<'_> {
        DwarfFunctionIterator {
            units: self.cell.get().units(self.bcsymbolmap.as_deref()),
            functions: Vec::new().into_iter(),
            seen_ranges: BTreeSet::new(),
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

impl<'data, 'session> DebugSession<'session> for DwarfDebugSession<'data> {
    type Error = DwarfError;
    type FunctionIterator = DwarfFunctionIterator<'session>;
    type FileIterator = DwarfFileIterator<'session>;

    fn functions(&'session self) -> Self::FunctionIterator {
        self.functions()
    }

    fn files(&'session self) -> Self::FileIterator {
        self.files()
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

fn resolve_byte_name<'s>(bcsymbolmap: Option<&'s BcSymbolMap<'s>>, s: &'s [u8]) -> &'s [u8] {
    bcsymbolmap
        .and_then(|b| b.resolve_opt(s))
        .map(AsRef::as_ref)
        .unwrap_or(s)
}

fn resolve_cow_name<'s>(bcsymbolmap: Option<&'s BcSymbolMap<'s>>, s: Cow<'s, str>) -> Cow<'s, str> {
    bcsymbolmap
        .and_then(|b| b.resolve_opt(s.as_bytes()))
        .map(Cow::Borrowed)
        .unwrap_or(s)
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
    seen_ranges: BTreeSet<(u64, u64)>,
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

            self.functions = match unit.functions(&mut self.seen_ranges) {
                Ok(functions) => functions.into_iter(),
                Err(error) => return Some(Err(error)),
            };
        }

        self.finished = true;
        None
    }
}

impl std::iter::FusedIterator for DwarfFunctionIterator<'_> {}
