use std::borrow::Cow;
use std::rc::Rc;

use failure::Fail;
use fallible_iterator::FallibleIterator;
use fnv::FnvBuildHasher;
use gimli::AttributeValue;
use lru_cache::LruCache;

use symbolic_common::{derive_failure, AsSelf, Language, Name, SelfCell};

use crate::base::*;

#[doc(hidden)]
pub use gimli;
pub use gimli::RunTimeEndian as Endian;

const ABBREV_CACHE_SIZE: usize = 30;

type Slice<'a> = gimli::EndianSlice<'a, Endian>;
type DebugAbbrev<'a> = gimli::DebugAbbrev<Slice<'a>>;
type DebugInfo<'a> = gimli::DebugInfo<Slice<'a>>;
type DebugLine<'a> = gimli::DebugLine<Slice<'a>>;
type DebugRanges<'a> = gimli::DebugRanges<Slice<'a>>;
type DebugRngLists<'a> = gimli::DebugRngLists<Slice<'a>>;
type DebugStr<'a> = gimli::DebugStr<Slice<'a>>;
type RangeLists<'a> = gimli::RangeLists<Slice<'a>>;

type CompilationUnitHeader<'a> = gimli::CompilationUnitHeader<Slice<'a>>;
type Die<'d, 'u> = gimli::DebuggingInformationEntry<'u, 'u, Slice<'d>, usize>;
type UnitOffset = gimli::UnitOffset<usize>;
type DebugInfoOffset = gimli::DebugInfoOffset<usize>;

type IncompleteLineNumberProgram<'a> = gimli::IncompleteLineNumberProgram<Slice<'a>>;
type LineNumberProgramHeader<'a> = gimli::LineNumberProgramHeader<Slice<'a>>;

/// Variants of `DwarfError`.
#[derive(Clone, Copy, Debug, Eq, Fail, PartialEq)]
pub enum DwarfErrorKind {
    #[fail(display = "missing required {} section", _0)]
    MissingSection(DwarfSection),
    #[fail(display = "missing compilation unit")]
    MissingCompileUnit,
    #[fail(display = "compilation unit for offset {} does not exist", _0)]
    InvalidUnitRef(usize),
    #[fail(display = "referenced file {} does not exist", _0)]
    InvalidFileRef(u64),
    #[fail(display = "unexpected inline function without parent")]
    UnexpectedInline,
    #[fail(display = "function with inverted address range")]
    InvertedFunctionRange,
    #[fail(display = "corrupted dwarf debug data")]
    CorruptedData,
    // #[fail(display = "corrupted dwarf debug data: {}", _0)]
    // CorruptedDataReason(&'static str),
    #[fail(display = "processing of dwarf debug info failed")]
    ProcessingFailed,
}

derive_failure!(DwarfError, DwarfErrorKind);

impl From<gimli::Error> for DwarfError {
    fn from(error: gimli::Error) -> Self {
        error.context(DwarfErrorKind::CorruptedData).into()
    }
}

/// Represents the name of a DWARF debug section.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DwarfSection {
    EhFrame,
    DebugFrame,
    DebugAbbrev,
    DebugAranges,
    DebugLine,
    DebugLoc,
    DebugPubNames,
    DebugRanges,
    DebugRngLists,
    DebugStr,
    DebugInfo,
    DebugTypes,
}

impl DwarfSection {
    /// Return the name of the section for debug purposes.
    pub fn name(self) -> &'static str {
        match self {
            DwarfSection::EhFrame => "eh_frame",
            DwarfSection::DebugFrame => "debug_frame",
            DwarfSection::DebugAbbrev => "debug_abbrev",
            DwarfSection::DebugAranges => "debug_aranges",
            DwarfSection::DebugLine => "debug_line",
            DwarfSection::DebugLoc => "debug_loc",
            DwarfSection::DebugPubNames => "debug_pubnames",
            DwarfSection::DebugRanges => "debug_ranges",
            DwarfSection::DebugRngLists => "debug_rnglists",
            DwarfSection::DebugStr => "debug_str",
            DwarfSection::DebugInfo => "debug_info",
            DwarfSection::DebugTypes => "debug_types",
        }
    }
}

impl std::fmt::Display for DwarfSection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

pub trait Dwarf<'data> {
    fn endianity(&self) -> Endian;

    fn raw_data(&self, section: DwarfSection) -> Option<(u64, &'data [u8])>;

    fn section_data(&self, section: DwarfSection) -> Option<(u64, Cow<'data, [u8]>)> {
        let (offset, data) = self.raw_data(section)?;
        Some((offset, Cow::Borrowed(data)))
    }

    fn has_section(&self, section: DwarfSection) -> bool {
        self.raw_data(section).is_some()
    }
}

#[derive(Clone, Debug)]
pub struct AbbrevCache {
    cache: LruCache<gimli::DebugAbbrevOffset<usize>, Rc<gimli::Abbreviations>, FnvBuildHasher>,
}

impl AbbrevCache {
    pub fn new() -> Self {
        Self::with_capacity(ABBREV_CACHE_SIZE)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let cache = LruCache::with_hasher(capacity, FnvBuildHasher::default());
        AbbrevCache { cache }
    }

    pub fn get(
        &mut self,
        unit: &CompilationUnitHeader<'_>,
        debug_abbrev: &DebugAbbrev<'_>,
    ) -> Result<Rc<gimli::Abbreviations>, DwarfError> {
        let offset = unit.debug_abbrev_offset();
        if let Some(abbrev) = self.cache.get_mut(&offset) {
            Ok(abbrev.clone())
        } else {
            let abbrev = Rc::new(unit.abbreviations(&debug_abbrev)?);
            self.cache.insert(offset, abbrev.clone());
            Ok(abbrev)
        }
    }
}

impl Default for AbbrevCache {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
struct AbbrevCacheHandle<'a> {
    cache: &'a mut AbbrevCache,
    debug_abbrev: &'a DebugAbbrev<'a>,
}

impl<'a> AbbrevCacheHandle<'a> {
    pub fn new(cache: &'a mut AbbrevCache, debug_abbrev: &'a DebugAbbrev<'_>) -> Self {
        AbbrevCacheHandle {
            cache,
            debug_abbrev,
        }
    }

    pub fn get(
        &mut self,
        unit: &CompilationUnitHeader<'_>,
    ) -> Result<Rc<gimli::Abbreviations>, DwarfError> {
        self.cache.get(unit, self.debug_abbrev)
    }
}

#[derive(Debug)]
struct DwarfRow {
    address: u64,
    file_index: u64,
    line: Option<u64>,
}

#[derive(Debug)]
struct DwarfSequence {
    start: u64,
    end: u64,
    rows: Vec<DwarfRow>,
}

#[derive(Debug)]
struct DwarfLineProgram<'a> {
    header: LineNumberProgramHeader<'a>,
    sequences: Vec<DwarfSequence>,
}

impl<'a> DwarfLineProgram<'a> {
    fn prepare(program: IncompleteLineNumberProgram<'a>) -> Result<Self, DwarfError> {
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

    pub fn get_fileinfo(&self, index: u64) -> Result<FileInfo<'a>, DwarfError> {
        let file = self
            .header
            .file(index)
            .ok_or_else(|| DwarfErrorKind::InvalidFileRef(index))?;

        Ok(FileInfo {
            dir: file
                .directory(&self.header)
                .map(|dir| String::from_utf8_lossy(dir.slice()))
                .unwrap_or_default(),
            name: String::from_utf8_lossy(file.path_name().slice()),
        })
    }

    pub fn get_rows(&self, range: &gimli::Range) -> &[DwarfRow] {
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

#[derive(Debug)]
struct DwarfUnit<'d, 'a> {
    header: &'a CompilationUnitHeader<'d>,
    info: &'a DwarfInfo<'d>,
    base_address: u64,
    comp_dir: Cow<'d, str>,
    language: Language,
    line_program: DwarfLineProgram<'d>,
    abbrev: Rc<gimli::Abbreviations>,
}

impl<'d, 'a> DwarfUnit<'d, 'a> {
    fn parse(
        header: &'a CompilationUnitHeader<'d>,
        info: &'a DwarfInfo<'d>,
        abbrevs: &mut AbbrevCacheHandle<'_>,
    ) -> Result<Option<Self>, DwarfError> {
        let abbrev = abbrevs.get(header)?;

        // Access the compilation unit, which must be the top level DIE
        let mut entries = header.entries(&abbrev);
        let entry = match entries.next_dfs()? {
            Some((_, entry)) => entry,
            None => return Ok(None),
        };

        if entry.tag() != gimli::DW_TAG_compile_unit {
            return Err(DwarfErrorKind::MissingCompileUnit.into());
        }

        let base_address = match entry.attr_value(gimli::DW_AT_low_pc)? {
            Some(AttributeValue::Addr(addr)) => addr,
            _ => match entry.attr_value(gimli::DW_AT_entry_pc)? {
                Some(AttributeValue::Addr(addr)) => addr,
                _ => 0,
            },
        };

        // Skip units without without line ranges. They do not contain code.
        let line_offset = match entry.attr_value(gimli::DW_AT_stmt_list)? {
            Some(AttributeValue::DebugLineRef(offset)) => offset,
            _ => return Ok(None),
        };

        let comp_dir = entry
            .attr(gimli::DW_AT_comp_dir)?
            .and_then(|attr| attr.string_value(&info.debug_str));

        let comp_name = entry
            .attr(gimli::DW_AT_name)?
            .and_then(|attr| attr.string_value(&info.debug_str));

        let language = entry
            .attr(gimli::DW_AT_language)?
            .and_then(|attr| match attr.value() {
                AttributeValue::Language(lang) => Some(language_from_dwarf(lang)),
                _ => None,
            });

        let program =
            info.debug_line
                .program(line_offset, header.address_size(), comp_dir, comp_name)?;

        Ok(Some(DwarfUnit {
            header,
            info,
            base_address,
            comp_dir: comp_dir
                .map(|slice| String::from_utf8_lossy(slice.slice()))
                .unwrap_or_default(),
            language: language.unwrap_or(Language::Unknown),
            line_program: DwarfLineProgram::prepare(program)?,
            abbrev,
        }))
    }

    /// Resolves an entry and if found invokes a function to transform it.
    ///
    /// As this might resolve into cached information the data borrowed from
    /// abbrev can only be temporarily accessed in the callback.
    fn resolve_reference<T, F>(
        &self,
        attr_value: AttributeValue<Slice<'_>>,
        abbrevs: &mut AbbrevCacheHandle<'_>,
        f: F,
    ) -> Result<Option<T>, DwarfError>
    where
        F: FnOnce(&Die<'d, '_>, &mut AbbrevCacheHandle<'_>) -> Result<Option<T>, DwarfError>,
    {
        let (header, offset) = match attr_value {
            AttributeValue::UnitRef(offset) => (self.header, offset),
            AttributeValue::DebugInfoRef(offset) => self.info.find_unit_offset(offset)?,
            // TODO: There is probably more that can come back here.
            _ => return Ok(None),
        };

        let abbrev = abbrevs.get(header)?;
        let mut entries = header.entries_at_offset(&abbrev, offset)?;

        entries.next_entry()?;
        if let Some(entry) = entries.current() {
            f(entry, abbrevs)
        } else {
            Ok(None)
        }
    }

    fn parse_ranges(
        &self,
        entry: &Die<'_, '_>,
        buf: &mut Vec<gimli::Range>,
    ) -> Result<(Option<u64>, Option<u64>), DwarfError> {
        let mut tuple = <(_, _)>::default();
        let mut low_pc = None;
        let mut high_pc = None;
        let mut high_pc_rel = None;

        let mut attrs = entry.attrs();
        while let Some(attr) = attrs.next()? {
            match attr.name() {
                gimli::DW_AT_ranges => match attr.value() {
                    AttributeValue::RangeListsRef(offset) => {
                        let mut attrs = self.info.range_lists.ranges(
                            offset,
                            self.header.version(),
                            self.header.address_size(),
                            self.base_address,
                        )?;

                        while let Some(item) = attrs.next()? {
                            buf.push(item);
                        }
                    }
                    _ => unreachable!(),
                },
                gimli::DW_AT_low_pc => match attr.value() {
                    AttributeValue::Addr(addr) => low_pc = Some(addr),
                    _ => unreachable!(),
                },
                gimli::DW_AT_high_pc => match attr.value() {
                    AttributeValue::Addr(addr) => high_pc = Some(addr),
                    AttributeValue::Udata(size) => high_pc_rel = Some(size),
                    _ => unreachable!(),
                },
                gimli::DW_AT_call_line => match attr.value() {
                    AttributeValue::Udata(line) => tuple.0 = Some(line),
                    _ => unreachable!(),
                },
                gimli::DW_AT_call_file => match attr.value() {
                    AttributeValue::FileIndex(file) => tuple.1 = Some(file),
                    _ => unreachable!(),
                },
                _ => continue,
            }
        }

        // Found DW_AT_ranges, so early-exit here
        if !buf.is_empty() {
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

        buf.push(gimli::Range {
            begin: low_pc,
            end: high_pc,
        });

        Ok(tuple)
    }

    /// Resolves the function name of a debug entry.
    fn resolve_function_name(
        &self,
        entry: &Die<'d, '_>,
        abbrevs: &mut AbbrevCacheHandle<'_>,
    ) -> Result<Option<Cow<'d, str>>, DwarfError> {
        let mut attrs = entry.attrs();
        let mut fallback_name = None;
        let mut reference_target = None;

        while let Some(attr) = attrs.next()? {
            match attr.name() {
                // prioritize these.  If we get them, take them.
                gimli::DW_AT_linkage_name | gimli::DW_AT_MIPS_linkage_name => {
                    return Ok(attr
                        .string_value(&self.info.debug_str)
                        .map(|s| s.to_string_lossy()));
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
            return Ok(attr
                .string_value(&self.info.debug_str)
                .map(|s| s.to_string_lossy()));
        }

        if let Some(attr) = reference_target {
            return self.resolve_reference(attr.value(), abbrevs, |ref_entry, abbrevs| {
                self.resolve_function_name(ref_entry, abbrevs)
            });
        }

        Ok(None)
    }

    fn resolve_lines(
        &self,
        ranges: &[gimli::Range],
        lines: &mut Vec<LineInfo<'d>>,
    ) -> Result<(), DwarfError> {
        let mut last = None;
        for range in ranges {
            for row in self.line_program.get_rows(range) {
                let file = self.line_program.get_fileinfo(row.file_index)?;
                let line = row.line.unwrap_or(0);

                if let Some((last_file, last_line)) = last {
                    if last_file == row.file_index && last_line == line {
                        continue;
                    }
                }

                last = Some((row.file_index, line));
                lines.push(LineInfo {
                    address: row.address - self.info.load_address,
                    file,
                    line,
                });
            }
        }

        Ok(())
    }

    fn resolve_file(&self, file_id: u64) -> Result<FileInfo<'d>, DwarfError> {
        self.line_program.get_fileinfo(file_id)
    }
}

fn language_from_dwarf(language: gimli::DwLang) -> Language {
    match language {
        gimli::DW_LANG_C => Language::C,
        gimli::DW_LANG_C11 => Language::C,
        gimli::DW_LANG_C89 => Language::C,
        gimli::DW_LANG_C99 => Language::C,
        gimli::DW_LANG_C_plus_plus => Language::Cpp,
        gimli::DW_LANG_C_plus_plus_03 => Language::Cpp,
        gimli::DW_LANG_C_plus_plus_11 => Language::Cpp,
        gimli::DW_LANG_C_plus_plus_14 => Language::Cpp,
        gimli::DW_LANG_D => Language::D,
        gimli::DW_LANG_Go => Language::Go,
        gimli::DW_LANG_ObjC => Language::ObjC,
        gimli::DW_LANG_ObjC_plus_plus => Language::ObjCpp,
        gimli::DW_LANG_Rust => Language::Rust,
        gimli::DW_LANG_Swift => Language::Swift,
        _ => Language::Unknown,
    }
}

#[derive(Debug)]
pub struct DwarfData<'d> {
    endianity: Endian,
    debug_info: Cow<'d, [u8]>,
    debug_abbrev: Cow<'d, [u8]>,
    debug_line: Cow<'d, [u8]>,
    debug_str: Cow<'d, [u8]>,
    debug_ranges: Cow<'d, [u8]>,
    debug_rnglists: Cow<'d, [u8]>,
}

impl<'d> DwarfData<'d> {
    pub fn from_dwarf<D>(dwarf: &D) -> Result<Self, DwarfError>
    where
        D: Dwarf<'d>,
    {
        Ok(DwarfData {
            endianity: dwarf.endianity(),
            debug_info: dwarf
                .section_data(DwarfSection::DebugInfo)
                .ok_or_else(|| DwarfErrorKind::MissingSection(DwarfSection::DebugInfo))?
                .1,
            debug_abbrev: dwarf
                .section_data(DwarfSection::DebugAbbrev)
                .ok_or_else(|| DwarfErrorKind::MissingSection(DwarfSection::DebugAbbrev))?
                .1,
            debug_line: dwarf
                .section_data(DwarfSection::DebugLine)
                .ok_or_else(|| DwarfErrorKind::MissingSection(DwarfSection::DebugLine))?
                .1,
            debug_str: dwarf
                .section_data(DwarfSection::DebugStr)
                .unwrap_or_default()
                .1,
            debug_ranges: dwarf
                .section_data(DwarfSection::DebugRanges)
                .unwrap_or_default()
                .1,
            debug_rnglists: dwarf
                .section_data(DwarfSection::DebugRngLists)
                .unwrap_or_default()
                .1,
        })
    }
}

struct FunctionStack<'a>(Vec<(isize, Function<'a>)>);

impl<'a> FunctionStack<'a> {
    pub fn new() -> Self {
        FunctionStack(Vec::with_capacity(16))
    }

    pub fn push(&mut self, depth: isize, function: Function<'a>) {
        self.0.push((depth, function));
    }

    pub fn peek_mut(&mut self) -> Option<&mut Function<'a>> {
        self.0.last_mut().map(|&mut (_, ref mut function)| function)
    }

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

#[derive(Debug)]
pub struct DwarfInfo<'d> {
    units: Vec<CompilationUnitHeader<'d>>,
    debug_abbrev: DebugAbbrev<'d>,
    debug_line: DebugLine<'d>,
    debug_str: DebugStr<'d>,
    range_lists: RangeLists<'d>,
    load_address: u64,
}

impl<'d> DwarfInfo<'d> {
    pub fn parse(data: &'d DwarfData<'_>, load_address: u64) -> Result<Self, DwarfError> {
        Ok(DwarfInfo {
            units: DebugInfo::new(&data.debug_info, data.endianity)
                .units()
                .collect()?,
            debug_abbrev: DebugAbbrev::new(&data.debug_abbrev, data.endianity),
            debug_line: DebugLine::new(&data.debug_line, data.endianity),
            debug_str: DebugStr::new(&data.debug_str, data.endianity),
            range_lists: RangeLists::new(
                DebugRanges::new(&data.debug_ranges, data.endianity),
                DebugRngLists::new(&data.debug_rnglists, data.endianity),
            )?,
            load_address,
        })
    }

    pub fn functions(
        &self,
        symbol_map: &SymbolMap<'d>,
        abbrev_cache: &mut AbbrevCache,
    ) -> Result<Vec<Function<'d>>, DwarfError> {
        let mut abbrevs = AbbrevCacheHandle::new(abbrev_cache, &self.debug_abbrev);
        let mut range_buf = Vec::new();
        let mut line_buf = Vec::new();
        let mut functions = Vec::new();

        for header in self.units.iter() {
            let batch_start = functions.len();

            let unit = match DwarfUnit::parse(header, self, &mut abbrevs)? {
                Some(unit) => unit,
                None => continue,
            };

            let mut depth = 0;
            let mut skipped_depth = None;

            let mut stack = FunctionStack::new();
            let mut entries = unit.header.entries(&unit.abbrev);
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
                    gimli::DW_TAG_subprogram => false,
                    gimli::DW_TAG_inlined_subroutine => true,
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
                    symbol_map
                        .lookup_range(function_address, function_address + function_size)
                        .and_then(|symbol| symbol.name.clone())
                } else {
                    None
                };

                let name = match symbol_name {
                    Some(name) => Some(name),
                    None => unit.resolve_function_name(entry, &mut abbrevs)?,
                };

                // Avoid constant allocations by collecting repeatedly into the same buffer and
                // draining the results out of it. This keeps the original buffer allocated and
                // allows for a single allocation per call to `resolve_lines`.
                unit.resolve_lines(&range_buf, &mut line_buf)?;

                let function = Function {
                    address: function_address,
                    size: function_size,
                    name: Name::with_language(name.unwrap_or_default(), unit.language),
                    compilation_dir: unit.comp_dir.clone(),
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
                        let file = unit.resolve_file(file_id)?;

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
                        };
                    }
                }

                stack.push(depth, function)
            }

            // We're done, flush the remaining stack.
            stack.flush(0, &mut functions);

            // Units are sorted by their address range in DWARF, but the functions within may occurr
            // in any order. Sort the batch that was just written, therefore.
            dmsort::sort_by_key(&mut functions[batch_start..], |f| f.address)
        }

        Ok(functions)
    }

    fn find_unit_offset(
        &self,
        offset: DebugInfoOffset,
    ) -> Result<(&CompilationUnitHeader<'d>, UnitOffset), DwarfError> {
        let idx = match self.units.binary_search_by_key(&offset.0, |x| x.offset().0) {
            Ok(idx) => idx,
            Err(0) => return Err(DwarfErrorKind::InvalidUnitRef(offset.0).into()),
            Err(next_idx) => next_idx - 1,
        };

        if let Some(header) = self.units.get(idx) {
            if let Some(unit_offset) = offset.to_unit_offset(header) {
                return Ok((header, unit_offset));
            }
        }

        Err(DwarfErrorKind::InvalidUnitRef(offset.0).into())
    }
}

impl<'slf: 'd, 'd> AsSelf<'slf> for DwarfInfo<'d> {
    type Ref = DwarfInfo<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        self
    }
}

pub struct DwarfDebugSession<'data> {
    cell: SelfCell<Box<DwarfData<'data>>, DwarfInfo<'data>>,
    symbols: SymbolMap<'data>,
    abbrev_cache: AbbrevCache,
}

impl<'d> DwarfDebugSession<'d> {
    pub fn parse(
        data: DwarfData<'d>,
        symbols: SymbolMap<'d>,
        load_address: u64,
    ) -> Result<Self, DwarfError> {
        let cell = SelfCell::try_new(Box::new(data), |d| {
            DwarfInfo::parse(unsafe { &*d }, load_address)
        })?;

        Ok(DwarfDebugSession {
            cell,
            symbols,
            abbrev_cache: AbbrevCache::new(),
        })
    }
}

impl<'slf: 'd, 'd> AsSelf<'slf> for DwarfDebugSession<'d> {
    type Ref = DwarfDebugSession<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        self
    }
}

impl<'d> DebugSession for DwarfDebugSession<'d> {
    type Error = DwarfError;

    fn functions(&mut self) -> Result<Vec<Function<'_>>, Self::Error> {
        self.cell
            .get()
            .functions(&self.symbols, &mut self.abbrev_cache)
    }
}
