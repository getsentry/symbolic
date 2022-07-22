//! Support for Breakpad ASCII symbols, used by the Breakpad and Crashpad libraries.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::ops::Range;
use std::str;

use thiserror::Error;

use symbolic_common::{Arch, AsSelf, CodeId, DebugId, Language, Name, NameMangling};

use crate::base::*;
use crate::function_builder::FunctionBuilder;
use crate::Parse;

#[derive(Clone, Debug)]
struct LineOffsets<'data> {
    data: &'data [u8],
    finished: bool,
    index: usize,
}

impl<'data> LineOffsets<'data> {
    #[inline]
    fn new(data: &'data [u8]) -> Self {
        Self {
            data,
            finished: false,
            index: 0,
        }
    }
}

impl Default for LineOffsets<'_> {
    #[inline]
    fn default() -> Self {
        Self {
            data: &[],
            finished: true,
            index: 0,
        }
    }
}

impl<'data> Iterator for LineOffsets<'data> {
    type Item = (usize, &'data [u8]);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        match self.data.iter().position(|b| *b == b'\n') {
            None => {
                if self.finished {
                    None
                } else {
                    self.finished = true;
                    Some((self.index, self.data))
                }
            }
            Some(index) => {
                let mut data = &self.data[..index];
                if index > 0 && data[index - 1] == b'\r' {
                    data = &data[..index - 1];
                }

                let item = Some((self.index, data));
                self.index += index + 1;
                self.data = &self.data[index + 1..];
                item
            }
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.finished {
            (0, Some(0))
        } else {
            (1, Some(self.data.len() + 1))
        }
    }
}

impl std::iter::FusedIterator for LineOffsets<'_> {}

#[allow(missing_docs)]
#[derive(Clone, Debug, Default)]
pub struct Lines<'data>(LineOffsets<'data>);

impl<'data> Lines<'data> {
    #[inline]
    #[allow(missing_docs)]
    pub fn new(data: &'data [u8]) -> Self {
        Self(LineOffsets::new(data))
    }
}

impl<'data> Iterator for Lines<'data> {
    type Item = &'data [u8];

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|tup| tup.1)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl std::iter::FusedIterator for Lines<'_> {}

/// Length at which the breakpad header will be capped.
///
/// This is a protection against reading an entire breakpad file at once if the first characters do
/// not contain a valid line break.
const BREAKPAD_HEADER_CAP: usize = 320;

/// Placeholder used for missing function or symbol names.
const UNKNOWN_NAME: &str = "<unknown>";

/// The error type for [`BreakpadError`].
#[non_exhaustive]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BreakpadErrorKind {
    /// The symbol header (`MODULE` record) is missing.
    InvalidMagic,

    /// A part of the file is not encoded in valid UTF-8.
    BadEncoding,

    /// A record violates the Breakpad symbol syntax.
    #[deprecated(note = "This is now covered by the Parse variant")]
    BadSyntax,

    /// Parsing of a record failed.
    ///
    /// The field exists only for API compatibility reasons.
    Parse(&'static str),

    /// The module ID is invalid.
    InvalidModuleId,

    /// The architecture is invalid.
    InvalidArchitecture,
}

impl fmt::Display for BreakpadErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMagic => write!(f, "missing breakpad symbol header"),
            Self::BadEncoding => write!(f, "bad utf-8 sequence"),
            Self::Parse(_) => write!(f, "parsing error"),
            Self::InvalidModuleId => write!(f, "invalid module id"),
            Self::InvalidArchitecture => write!(f, "invalid architecture"),
            _ => Ok(()),
        }
    }
}

/// An error when dealing with [`BreakpadObject`](struct.BreakpadObject.html).
#[derive(Debug, Error)]
#[error("{kind}")]
pub struct BreakpadError {
    kind: BreakpadErrorKind,
    #[source]
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl BreakpadError {
    /// Creates a new Breakpad error from a known kind of error as well as an arbitrary error
    /// payload.
    fn new<E>(kind: BreakpadErrorKind, source: E) -> Self
    where
        E: Into<Box<dyn Error + Send + Sync>>,
    {
        let source = Some(source.into());
        Self { kind, source }
    }

    /// Returns the corresponding [`BreakpadErrorKind`] for this error.
    pub fn kind(&self) -> BreakpadErrorKind {
        self.kind
    }
}

impl From<BreakpadErrorKind> for BreakpadError {
    fn from(kind: BreakpadErrorKind) -> Self {
        Self { kind, source: None }
    }
}

impl From<str::Utf8Error> for BreakpadError {
    fn from(e: str::Utf8Error) -> Self {
        Self::new(BreakpadErrorKind::BadEncoding, e)
    }
}

impl From<parsing::ParseBreakpadError> for BreakpadError {
    fn from(e: parsing::ParseBreakpadError) -> Self {
        Self::new(BreakpadErrorKind::Parse(""), e)
    }
}

// TODO(ja): Test the parser

/// A [module record], constituting the header of a Breakpad file.
///
/// Example: `MODULE Linux x86 D3096ED481217FD4C16B29CD9BC208BA0 firefox-bin`
///
/// [module record]: https://github.com/google/breakpad/blob/master/docs/symbol_files.md#module-records
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BreakpadModuleRecord<'d> {
    /// Name of the operating system.
    pub os: &'d str,
    /// Name of the CPU architecture.
    pub arch: &'d str,
    /// Breakpad identifier.
    pub id: &'d str,
    /// Name of the original file.
    ///
    /// This usually corresponds to the debug file (such as a PDB), but might not necessarily have a
    /// special file extension, such as for MachO dSYMs which share the same name as their code
    /// file.
    pub name: &'d str,
}

impl<'d> BreakpadModuleRecord<'d> {
    /// Parses a module record from a single line.
    pub fn parse(data: &'d [u8]) -> Result<Self, BreakpadError> {
        let string = str::from_utf8(data)?;
        Ok(parsing::module_record_final(string.trim())?)
    }
}

/// An information record.
///
/// This record type is not documented, but appears in Breakpad symbols after the header. It seems
/// that currently only a `CODE_ID` scope is used, which contains the platform-dependent original
/// code identifier of an object file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BreakpadInfoRecord<'d> {
    /// Information on the code file.
    CodeId {
        /// Identifier of the code file.
        code_id: &'d str,
        /// File name of the code file.
        code_file: &'d str,
    },
    /// Any other INFO record.
    Other {
        /// The scope of this info record.
        scope: &'d str,
        /// The information for this scope.
        info: &'d str,
    },
}

impl<'d> BreakpadInfoRecord<'d> {
    /// Parses an info record from a single line.
    pub fn parse(data: &'d [u8]) -> Result<Self, BreakpadError> {
        let string = str::from_utf8(data)?;
        Ok(parsing::info_record_final(string.trim())?)
    }
}

/// An iterator over info records in a Breakpad object.
#[derive(Clone, Debug)]
pub struct BreakpadInfoRecords<'d> {
    lines: Lines<'d>,
    finished: bool,
}

impl<'d> Iterator for BreakpadInfoRecords<'d> {
    type Item = Result<BreakpadInfoRecord<'d>, BreakpadError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        for line in &mut self.lines {
            if line.starts_with(b"MODULE ") {
                continue;
            }

            // Fast path: INFO records come right after the header.
            if !line.starts_with(b"INFO ") {
                break;
            }

            return Some(BreakpadInfoRecord::parse(line));
        }

        self.finished = true;
        None
    }
}

/// A [file record], specifying the path to a source code file.
///
/// The ID of this record is referenced by [`BreakpadLineRecord`]. File records are not necessarily
/// consecutive or sorted by their identifier. The Breakpad symbol writer might reuse original
/// identifiers from the source debug file when dumping symbols.
///
/// Example: `FILE 2 /home/jimb/mc/in/browser/app/nsBrowserApp.cpp`
///
/// [file record]: https://github.com/google/breakpad/blob/master/docs/symbol_files.md#file-records
/// [`LineRecord`]: struct.BreakpadLineRecord.html
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BreakpadFileRecord<'d> {
    /// Breakpad-internal identifier of the file.
    pub id: u64,
    /// The path to the source file, usually relative to the compilation directory.
    pub name: &'d str,
}

impl<'d> BreakpadFileRecord<'d> {
    /// Parses a file record from a single line.
    pub fn parse(data: &'d [u8]) -> Result<Self, BreakpadError> {
        let string = str::from_utf8(data)?;
        Ok(parsing::file_record_final(string.trim())?)
    }
}

/// An iterator over file records in a Breakpad object.
#[derive(Clone, Debug)]
pub struct BreakpadFileRecords<'d> {
    lines: Lines<'d>,
    finished: bool,
}

impl<'d> Iterator for BreakpadFileRecords<'d> {
    type Item = Result<BreakpadFileRecord<'d>, BreakpadError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        for line in &mut self.lines {
            if line.starts_with(b"MODULE ") || line.starts_with(b"INFO ") {
                continue;
            }

            // Fast path: FILE records come right after the header.
            if !line.starts_with(b"FILE ") {
                break;
            }

            return Some(BreakpadFileRecord::parse(line));
        }

        self.finished = true;
        None
    }
}

/// A map of file paths by their file ID.
pub type BreakpadFileMap<'d> = BTreeMap<u64, &'d str>;

/// An [inline origin record], specifying the function name of a function for which at least one
/// call to this function has been inlined.
///
/// The ID of this record is referenced by [`BreakpadInlineRecord`]. Inline origin records are not
/// necessarily consecutive or sorted by their identifier, and they don't have to be present in a
/// contiguous block in the file; they can be interspersed with FUNC records or other records.
///
/// Example: `INLINE_ORIGIN 1305 SharedLibraryInfo::Initialize()`
///
/// [inline origin record]: https://github.com/google/breakpad/blob/main/docs/symbol_files.md#inline_origin-records
/// [`BreakpadInlineRecord`]: struct.BreakpadInlineRecord.html
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BreakpadInlineOriginRecord<'d> {
    /// Breakpad-internal identifier of the function.
    pub id: u64,
    /// The function name.
    pub name: &'d str,
}

impl<'d> BreakpadInlineOriginRecord<'d> {
    /// Parses an inline origin record from a single line.
    pub fn parse(data: &'d [u8]) -> Result<Self, BreakpadError> {
        let string = str::from_utf8(data)?;
        Ok(parsing::inline_origin_record_final(string.trim())?)
    }
}

/// A map of function names by their inline origin ID.
pub type BreakpadInlineOriginMap<'d> = BTreeMap<u64, &'d str>;

/// A [public function symbol record].
///
/// Example: `PUBLIC m 2160 0 Public2_1`
///
/// [public function symbol record]: https://github.com/google/breakpad/blob/master/docs/symbol_files.md#public-records
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BreakpadPublicRecord<'d> {
    /// Whether this symbol was referenced multiple times.
    pub multiple: bool,
    /// The address of this symbol relative to the image base (load address).
    pub address: u64,
    /// The size of the parameters on the runtime stack.
    pub parameter_size: u64,
    /// The demangled function name of the symbol.
    pub name: &'d str,
}

impl<'d> BreakpadPublicRecord<'d> {
    /// Parses a public record from a single line.
    pub fn parse(data: &'d [u8]) -> Result<Self, BreakpadError> {
        let string = str::from_utf8(data)?;
        Ok(parsing::public_record_final(string.trim())?)
    }
}

/// An iterator over public symbol records in a Breakpad object.
#[derive(Clone, Debug)]
pub struct BreakpadPublicRecords<'d> {
    lines: Lines<'d>,
    finished: bool,
}

impl<'d> Iterator for BreakpadPublicRecords<'d> {
    type Item = Result<BreakpadPublicRecord<'d>, BreakpadError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        for line in &mut self.lines {
            // Fast path: PUBLIC records are always before stack records. Once we encounter the
            // first stack record, we can therefore exit.
            if line.starts_with(b"STACK ") {
                break;
            }

            if !line.starts_with(b"PUBLIC ") {
                continue;
            }

            return Some(BreakpadPublicRecord::parse(line));
        }

        self.finished = true;
        None
    }
}

/// A [function record] including line information.
///
/// Example: `FUNC m c184 30 0 nsQueryInterfaceWithError::operator()(nsID const&, void**) const`
///
/// [function record]: https://github.com/google/breakpad/blob/master/docs/symbol_files.md#func-records
#[derive(Clone, Default)]
pub struct BreakpadFuncRecord<'d> {
    /// Whether this function was referenced multiple times.
    pub multiple: bool,
    /// The start address of this function relative to the image base (load address).
    pub address: u64,
    /// The size of the code covered by this function's line records.
    pub size: u64,
    /// The size of the parameters on the runtime stack.
    pub parameter_size: u64,
    /// The demangled function name.
    pub name: &'d str,
    lines: Lines<'d>,
}

impl<'d> BreakpadFuncRecord<'d> {
    /// Parses a function record from a set of lines.
    ///
    /// The first line must contain the function record itself. The lines iterator may contain line
    /// records for this function, which are read until another record isencountered or the file
    /// ends.
    pub fn parse(data: &'d [u8], lines: Lines<'d>) -> Result<Self, BreakpadError> {
        let string = str::from_utf8(data)?;
        let mut record = parsing::func_record_final(string.trim())?;

        record.lines = lines;
        Ok(record)
    }

    /// Returns an iterator over line records associated to this function.
    pub fn lines(&self) -> BreakpadLineRecords<'d> {
        BreakpadLineRecords {
            lines: self.lines.clone(),
            finished: false,
        }
    }

    /// Returns the range of addresses covered by this record.
    pub fn range(&self) -> Range<u64> {
        self.address..self.address + self.size
    }
}

impl PartialEq for BreakpadFuncRecord<'_> {
    fn eq(&self, other: &BreakpadFuncRecord<'_>) -> bool {
        self.multiple == other.multiple
            && self.address == other.address
            && self.size == other.size
            && self.parameter_size == other.parameter_size
            && self.name == other.name
    }
}

impl Eq for BreakpadFuncRecord<'_> {}

impl fmt::Debug for BreakpadFuncRecord<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BreakpadFuncRecord")
            .field("multiple", &self.multiple)
            .field("address", &self.address)
            .field("size", &self.size)
            .field("parameter_size", &self.parameter_size)
            .field("name", &self.name)
            .finish()
    }
}

/// An iterator over function records in a Breakpad object.
#[derive(Clone, Debug)]
pub struct BreakpadFuncRecords<'d> {
    lines: Lines<'d>,
    finished: bool,
}

impl<'d> Iterator for BreakpadFuncRecords<'d> {
    type Item = Result<BreakpadFuncRecord<'d>, BreakpadError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        for line in &mut self.lines {
            // Fast path: FUNC records are always before stack records. Once we encounter the
            // first stack record, we can therefore exit.
            if line.starts_with(b"STACK ") {
                break;
            }

            if !line.starts_with(b"FUNC ") {
                continue;
            }

            return Some(BreakpadFuncRecord::parse(line, self.lines.clone()));
        }

        self.finished = true;
        None
    }
}

/// A [line record] associated to a `BreakpadFunctionRecord`.
///
/// Line records are so frequent in a Breakpad symbol file that they do not have a record
/// identifier. They immediately follow the [`BreakpadFuncRecord`] that they belong to. Thus, an
/// iterator over line records can be obtained from the function record.
///
/// Example: `c184 7 59 4`
///
/// [line record]: https://github.com/google/breakpad/blob/master/docs/symbol_files.md#line-records
/// [`BreakpadFuncRecord`]: struct.BreakpadFuncRecord.html
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BreakpadLineRecord {
    /// The start address for this line relative to the image base (load address).
    pub address: u64,
    /// The size of the code covered by this line record.
    pub size: u64,
    /// The line number (zero means no line number).
    pub line: u64,
    /// Identifier of the [`BreakpadFileRecord`] specifying the file name.
    pub file_id: u64,
}

impl BreakpadLineRecord {
    /// Parses a line record from a single line.
    pub fn parse(data: &[u8]) -> Result<Self, BreakpadError> {
        let string = str::from_utf8(data)?;
        Ok(parsing::line_record_final(string.trim())?)
    }

    /// Resolves the filename for this record in the file map.
    pub fn filename<'d>(&self, file_map: &BreakpadFileMap<'d>) -> Option<&'d str> {
        file_map.get(&self.file_id).cloned()
    }

    /// Returns the range of addresses covered by this record.
    pub fn range(&self) -> Range<u64> {
        self.address..self.address + self.size
    }
}

/// An iterator over line records in a `BreakpadFunctionRecord`.
#[derive(Clone, Debug)]
pub struct BreakpadLineRecords<'d> {
    lines: Lines<'d>,
    finished: bool,
}

impl<'d> Iterator for BreakpadLineRecords<'d> {
    type Item = Result<BreakpadLineRecord, BreakpadError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        for line in &mut self.lines {
            // Stop parsing LINE records once other expected records are encountered.
            if line.starts_with(b"FUNC ")
                || line.starts_with(b"PUBLIC ")
                || line.starts_with(b"STACK ")
            {
                break;
            }

            // There might be empty lines throughout the file (or at the end). This is the only
            // iterator that cannot rely on a record identifier, so we have to explicitly skip empty
            // lines.
            if line.is_empty() {
                continue;
            }

            let record = match BreakpadLineRecord::parse(line) {
                Ok(record) => record,
                Err(error) => return Some(Err(error)),
            };

            // Skip line records for empty ranges. These do not carry any information.
            if record.size > 0 {
                return Some(Ok(record));
            }
        }

        self.finished = true;
        None
    }
}

/// An [inline record] associated with a `BreakpadFunctionRecord`.
///
/// Inline records are so frequent in a Breakpad symbol file that they do not have a record
/// identifier. They immediately follow the [`BreakpadFuncRecord`] that they belong to. Thus, an
/// iterator over inline records can be obtained from the function record.
///
/// Example: `INLINE 1 61 1 2 7b60 3b4`
///
/// [inline record]: https://github.com/google/breakpad/blob/main/docs/symbol_files.md#inline-records
/// [`BreakpadFuncRecord`]: struct.BreakpadFuncRecord.html
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BreakpadInlineRecord {
    /// The depth of nested inline calls.
    pub inline_depth: u64,
    /// The line number of the call, in the parent function. Zero means no line number.
    pub call_site_line: u64,
    /// Identifier of the [`BreakpadFileRecord`] specifying the file name of the line of the call.
    pub call_site_file_id: u64,
    /// Identifier of the [`BreakpadInlineOriginRecord`] specifying the function name.
    pub origin_id: u64,
    /// A list of address ranges which contain the instructions for this inline call. Contains at
    /// least one element.
    pub address_ranges: Vec<BreakpadInlineAddressRange>,
}

impl BreakpadInlineRecord {
    /// Parses a line record from a single line.
    pub fn parse(data: &[u8]) -> Result<Self, BreakpadError> {
        let string = str::from_utf8(data)?;
        Ok(parsing::inline_record_final(string.trim())?)
    }
}

/// Identifies one contiguous slice of bytes / instruction addresses which is covered by a
/// [`BreakpadInlineRecord`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BreakpadInlineAddressRange {
    /// The start address for this address range relative to the image base (load address).
    pub address: u64,
    /// The length of the range, in bytes.
    pub size: u64,
}

impl BreakpadInlineAddressRange {
    /// Returns the range of addresses covered by this record.
    pub fn range(&self) -> Range<u64> {
        self.address..self.address + self.size
    }
}

/// A `STACK CFI` record. Usually associated with a [BreakpadStackCfiRecord].
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct BreakpadStackCfiDeltaRecord<'d> {
    /// The address covered by the record.
    pub address: u64,

    /// The unwind program rules.
    pub rules: &'d str,
}

impl<'d> BreakpadStackCfiDeltaRecord<'d> {
    /// Parses a single `STACK CFI` record.
    pub fn parse(data: &'d [u8]) -> Result<Self, BreakpadError> {
        let string = str::from_utf8(data)?;
        Ok(parsing::stack_cfi_delta_record_final(string.trim())?)
    }
}

/// A [call frame information record](https://github.com/google/breakpad/blob/master/docs/symbol_files.md#stack-cfi-records)
/// for platforms other than Windows x86.
///
/// This bundles together a `STACK CFI INIT` record and its associated `STACK CFI` records.
#[derive(Clone, Debug, Default)]
pub struct BreakpadStackCfiRecord<'d> {
    /// The starting address covered by this record.
    pub start: u64,

    /// The number of bytes covered by this record.
    pub size: u64,

    /// The unwind program rules in the `STACK CFI INIT` record.
    pub init_rules: &'d str,

    /// The `STACK CFI` records belonging to a single `STACK CFI INIT record.
    deltas: Lines<'d>,
}

impl<'d> BreakpadStackCfiRecord<'d> {
    /// Parses a `STACK CFI INIT` record from a single line.
    pub fn parse(data: &'d [u8]) -> Result<Self, BreakpadError> {
        let string = str::from_utf8(data)?;
        Ok(parsing::stack_cfi_record_final(string.trim())?)
    }

    /// Returns an iterator over this record's delta records.
    pub fn deltas(&self) -> BreakpadStackCfiDeltaRecords<'d> {
        BreakpadStackCfiDeltaRecords {
            lines: self.deltas.clone(),
        }
    }

    /// Returns the range of addresses covered by this record.
    pub fn range(&self) -> Range<u64> {
        self.start..self.start + self.size
    }
}

impl<'d> PartialEq for BreakpadStackCfiRecord<'d> {
    fn eq(&self, other: &Self) -> bool {
        self.start == other.start && self.size == other.size && self.init_rules == other.init_rules
    }
}

impl<'d> Eq for BreakpadStackCfiRecord<'d> {}

/// An iterator over stack cfi delta records associated with a particular
/// [`BreakpadStackCfiRecord`].
#[derive(Clone, Debug, Default)]
pub struct BreakpadStackCfiDeltaRecords<'d> {
    lines: Lines<'d>,
}

impl<'d> Iterator for BreakpadStackCfiDeltaRecords<'d> {
    type Item = Result<BreakpadStackCfiDeltaRecord<'d>, BreakpadError>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(line) = self.lines.next() {
            if line.starts_with(b"STACK CFI INIT") || !line.starts_with(b"STACK CFI") {
                self.lines = Lines::default();
            } else {
                return Some(BreakpadStackCfiDeltaRecord::parse(line));
            }
        }

        None
    }
}

/// Possible types of data held by a [`BreakpadStackWinRecord`], as listed in
/// <http://msdn.microsoft.com/en-us/library/bc5207xw%28VS.100%29.aspx>. Breakpad only deals with
/// types 0 (`FPO`) and 4 (`FrameData`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BreakpadStackWinRecordType {
    /// Frame pointer omitted; FPO info available.
    Fpo = 0,

    /// Kernel Trap frame.
    Trap = 1,

    /// Kernel Trap frame.
    Tss = 2,

    /// Standard EBP stack frame.
    Standard = 3,

    /// Frame pointer omitted; Frame data info available.
    FrameData = 4,

    /// Frame that does not have any debug info.
    Unknown = -1,
}

/// A [Windows stack frame record], used on x86.
///
/// Example: `STACK WIN 4 2170 14 1 0 0 0 0 0 1 $eip 4 + ^ = $esp $ebp 8 + = $ebp $ebp ^ =`
///
/// [Windows stack frame record]: https://github.com/google/breakpad/blob/master/docs/symbol_files.md#stack-win-records
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BreakpadStackWinRecord<'d> {
    /// The type of frame data this record holds.
    pub ty: BreakpadStackWinRecordType,

    /// The starting address covered by this record, relative to the module's load address.
    pub code_start: u32,

    /// The number of bytes covered by this record.
    pub code_size: u32,

    /// The size of the prologue machine code within the record's range in bytes.
    pub prolog_size: u16,

    /// The size of the epilogue machine code within the record's range in bytes.
    pub epilog_size: u16,

    /// The number of bytes this function expects to be passed as arguments.
    pub params_size: u32,

    /// The number of bytes used by this function to save callee-saves registers.
    pub saved_regs_size: u16,

    /// The number of bytes used to save this function's local variables.
    pub locals_size: u32,

    /// The maximum number of bytes pushed on the stack in the frame.
    pub max_stack_size: u32,

    /// Whether this function uses the base pointer register as a general-purpose register.
    ///
    /// This is only relevant for records of type 0 (`FPO`).
    pub uses_base_pointer: bool,

    /// A string describing a program for recovering the caller's register values.
    ///
    /// This is only expected to be present for records of type 4 (`FrameData`).
    pub program_string: Option<&'d str>,
}

impl<'d> BreakpadStackWinRecord<'d> {
    /// Parses a Windows stack record from a single line.
    pub fn parse(data: &'d [u8]) -> Result<Self, BreakpadError> {
        let string = str::from_utf8(data)?;
        Ok(parsing::stack_win_record_final(string.trim())?)
    }

    /// Returns the range of addresses covered by this record.
    pub fn code_range(&self) -> Range<u32> {
        self.code_start..self.code_start + self.code_size
    }
}

/// Stack frame information record used for stack unwinding and stackwalking.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BreakpadStackRecord<'d> {
    /// CFI stack record, used for all platforms other than Windows x86.
    Cfi(BreakpadStackCfiRecord<'d>),
    /// Windows stack record, used for x86 binaries.
    Win(BreakpadStackWinRecord<'d>),
}

impl<'d> BreakpadStackRecord<'d> {
    /// Parses a stack frame information record from a single line.
    pub fn parse(data: &'d [u8]) -> Result<Self, BreakpadError> {
        let string = str::from_utf8(data)?;
        Ok(parsing::stack_record_final(string.trim())?)
    }
}

/// An iterator over stack frame records in a Breakpad object.
#[derive(Clone, Debug)]
pub struct BreakpadStackRecords<'d> {
    lines: Lines<'d>,
    finished: bool,
}

impl<'d> BreakpadStackRecords<'d> {
    /// Creates an iterator over [`BreakpadStackRecord`]s contained in a slice of data.
    pub fn new(data: &'d [u8]) -> Self {
        Self {
            lines: Lines::new(data),
            finished: false,
        }
    }
}

impl<'d> Iterator for BreakpadStackRecords<'d> {
    type Item = Result<BreakpadStackRecord<'d>, BreakpadError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        while let Some(line) = self.lines.next() {
            if line.starts_with(b"STACK WIN") {
                return Some(BreakpadStackRecord::parse(line));
            }

            if line.starts_with(b"STACK CFI INIT") {
                return Some(BreakpadStackCfiRecord::parse(line).map(|mut r| {
                    r.deltas = self.lines.clone();
                    BreakpadStackRecord::Cfi(r)
                }));
            }
        }

        self.finished = true;
        None
    }
}

/// A Breakpad object file.
///
/// To process minidump crash reports without having to understand all sorts of native symbol
/// formats, the Breakpad processor uses a text-based symbol file format. It comprises records
/// describing the object file, functions and lines, public symbols, as well as unwind information
/// for stackwalking.
///
/// > The platform-specific symbol dumping tools parse the debugging information the compiler
/// > provides (whether as DWARF or STABS sections in an ELF file or as stand-alone PDB files), and
/// > write that information back out in the Breakpad symbol file format. This format is much
/// > simpler and less detailed than compiler debugging information, and values legibility over
/// > compactness.
///
/// The full documentation resides [here](https://chromium.googlesource.com/breakpad/breakpad/+/refs/heads/master/docs/symbol_files.md).
pub struct BreakpadObject<'data> {
    id: DebugId,
    arch: Arch,
    module: BreakpadModuleRecord<'data>,
    data: &'data [u8],
}

impl<'data> BreakpadObject<'data> {
    /// Tests whether the buffer could contain a Breakpad object.
    pub fn test(data: &[u8]) -> bool {
        data.starts_with(b"MODULE ")
    }

    /// Tries to parse a Breakpad object from the given slice.
    pub fn parse(data: &'data [u8]) -> Result<Self, BreakpadError> {
        // Ensure that we do not read the entire file at once.
        let header = if data.len() > BREAKPAD_HEADER_CAP {
            match str::from_utf8(&data[..BREAKPAD_HEADER_CAP]) {
                Ok(_) => &data[..BREAKPAD_HEADER_CAP],
                Err(e) => match e.error_len() {
                    None => &data[..e.valid_up_to()],
                    Some(_) => return Err(e.into()),
                },
            }
        } else {
            data
        };

        let first_line = header.split(|b| *b == b'\n').next().unwrap_or_default();
        let module = BreakpadModuleRecord::parse(first_line)?;

        Ok(BreakpadObject {
            id: module
                .id
                .parse()
                .map_err(|_| BreakpadErrorKind::InvalidModuleId)?,
            arch: module
                .arch
                .parse()
                .map_err(|_| BreakpadErrorKind::InvalidArchitecture)?,
            module,
            data,
        })
    }

    /// The container file format, which is always `FileFormat::Breakpad`.
    pub fn file_format(&self) -> FileFormat {
        FileFormat::Breakpad
    }

    /// The code identifier of this object.
    pub fn code_id(&self) -> Option<CodeId> {
        for result in self.info_records().flatten() {
            if let BreakpadInfoRecord::CodeId { code_id, .. } = result {
                if !code_id.is_empty() {
                    return Some(CodeId::new(code_id.into()));
                }
            }
        }

        None
    }

    /// The debug information identifier of this object.
    pub fn debug_id(&self) -> DebugId {
        self.id
    }

    /// The CPU architecture of this object.
    pub fn arch(&self) -> Arch {
        self.arch
    }

    /// The debug file name of this object.
    ///
    /// This is the name of the original debug file that was used to create the Breakpad file. On
    /// Windows, this will have a `.pdb` extension, on other platforms that name is likely
    /// equivalent to the name of the code file (shared library or executable).
    pub fn name(&self) -> &'data str {
        self.module.name
    }

    /// The kind of this object.
    pub fn kind(&self) -> ObjectKind {
        ObjectKind::Debug
    }

    /// The address at which the image prefers to be loaded into memory.
    ///
    /// When Breakpad symbols are written, all addresses are rebased relative to the load address.
    /// Since the original load address is not stored in the file, it is assumed as zero.
    pub fn load_address(&self) -> u64 {
        0 // Breakpad rebases all addresses when dumping symbols
    }

    /// Determines whether this object exposes a public symbol table.
    pub fn has_symbols(&self) -> bool {
        self.public_records().next().is_some()
    }

    /// Returns an iterator over symbols in the public symbol table.
    pub fn symbols(&self) -> BreakpadSymbolIterator<'data> {
        BreakpadSymbolIterator {
            records: self.public_records(),
        }
    }

    /// Returns an ordered map of symbols in the symbol table.
    pub fn symbol_map(&self) -> SymbolMap<'data> {
        self.symbols().collect()
    }

    /// Determines whether this object contains debug information.
    pub fn has_debug_info(&self) -> bool {
        self.func_records().next().is_some()
    }

    /// Constructs a debugging session.
    ///
    /// A debugging session loads certain information from the object file and creates caches for
    /// efficient access to various records in the debug information. Since this can be quite a
    /// costly process, try to reuse the debugging session as long as possible.
    ///
    /// Constructing this session will also work if the object does not contain debugging
    /// information, in which case the session will be a no-op. This can be checked via
    /// [`has_debug_info`](struct.BreakpadObject.html#method.has_debug_info).
    pub fn debug_session(&self) -> Result<BreakpadDebugSession<'data>, BreakpadError> {
        Ok(BreakpadDebugSession {
            file_map: self.file_map(),
            lines: Lines::new(self.data),
        })
    }

    /// Determines whether this object contains stack unwinding information.
    pub fn has_unwind_info(&self) -> bool {
        self.stack_records().next().is_some()
    }

    /// Determines whether this object contains embedded source.
    pub fn has_sources(&self) -> bool {
        false
    }

    /// Determines whether this object is malformed and was only partially parsed
    pub fn is_malformed(&self) -> bool {
        false
    }

    /// Returns an iterator over info records.
    pub fn info_records(&self) -> BreakpadInfoRecords<'data> {
        BreakpadInfoRecords {
            lines: Lines::new(self.data),
            finished: false,
        }
    }

    /// Returns an iterator over file records.
    pub fn file_records(&self) -> BreakpadFileRecords<'data> {
        BreakpadFileRecords {
            lines: Lines::new(self.data),
            finished: false,
        }
    }

    /// Returns a map for file name lookups by id.
    pub fn file_map(&self) -> BreakpadFileMap<'data> {
        self.file_records()
            .filter_map(Result::ok)
            .map(|file| (file.id, file.name))
            .collect()
    }

    /// Returns an iterator over public symbol records.
    pub fn public_records(&self) -> BreakpadPublicRecords<'data> {
        BreakpadPublicRecords {
            lines: Lines::new(self.data),
            finished: false,
        }
    }

    /// Returns an iterator over function records.
    pub fn func_records(&self) -> BreakpadFuncRecords<'data> {
        BreakpadFuncRecords {
            lines: Lines::new(self.data),
            finished: false,
        }
    }

    /// Returns an iterator over stack frame records.
    pub fn stack_records(&self) -> BreakpadStackRecords<'data> {
        BreakpadStackRecords {
            lines: Lines::new(self.data),
            finished: false,
        }
    }

    /// Returns the raw data of the Breakpad file.
    pub fn data(&self) -> &'data [u8] {
        self.data
    }
}

impl fmt::Debug for BreakpadObject<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BreakpadObject")
            .field("code_id", &self.code_id())
            .field("debug_id", &self.debug_id())
            .field("arch", &self.arch())
            .field("name", &self.name())
            .field("has_symbols", &self.has_symbols())
            .field("has_debug_info", &self.has_debug_info())
            .field("has_unwind_info", &self.has_unwind_info())
            .field("is_malformed", &self.is_malformed())
            .finish()
    }
}

impl<'slf, 'data: 'slf> AsSelf<'slf> for BreakpadObject<'data> {
    type Ref = BreakpadObject<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        self
    }
}

impl<'data> Parse<'data> for BreakpadObject<'data> {
    type Error = BreakpadError;

    fn test(data: &[u8]) -> bool {
        Self::test(data)
    }

    fn parse(data: &'data [u8]) -> Result<Self, BreakpadError> {
        Self::parse(data)
    }
}

impl<'data: 'object, 'object> ObjectLike<'data, 'object> for BreakpadObject<'data> {
    type Error = BreakpadError;
    type Session = BreakpadDebugSession<'data>;
    type SymbolIterator = BreakpadSymbolIterator<'data>;

    fn file_format(&self) -> FileFormat {
        self.file_format()
    }

    fn code_id(&self) -> Option<CodeId> {
        self.code_id()
    }

    fn debug_id(&self) -> DebugId {
        self.debug_id()
    }

    fn arch(&self) -> Arch {
        self.arch()
    }

    fn kind(&self) -> ObjectKind {
        self.kind()
    }

    fn load_address(&self) -> u64 {
        self.load_address()
    }

    fn has_symbols(&self) -> bool {
        self.has_symbols()
    }

    fn symbols(&self) -> Self::SymbolIterator {
        self.symbols()
    }

    fn symbol_map(&self) -> SymbolMap<'data> {
        self.symbol_map()
    }

    fn has_debug_info(&self) -> bool {
        self.has_debug_info()
    }

    fn debug_session(&self) -> Result<Self::Session, Self::Error> {
        self.debug_session()
    }

    fn has_unwind_info(&self) -> bool {
        self.has_unwind_info()
    }

    fn has_sources(&self) -> bool {
        self.has_sources()
    }

    fn is_malformed(&self) -> bool {
        self.is_malformed()
    }
}

/// An iterator over symbols in the Breakpad object.
///
/// Returned by [`BreakpadObject::symbols`](struct.BreakpadObject.html#method.symbols).
pub struct BreakpadSymbolIterator<'data> {
    records: BreakpadPublicRecords<'data>,
}

impl<'data> Iterator for BreakpadSymbolIterator<'data> {
    type Item = Symbol<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        self.records.find_map(Result::ok).map(|record| Symbol {
            name: Some(Cow::Borrowed(record.name)),
            address: record.address,
            size: 0,
        })
    }
}

/// Debug session for Breakpad objects.
pub struct BreakpadDebugSession<'data> {
    file_map: BreakpadFileMap<'data>,
    lines: Lines<'data>,
}

impl<'data> BreakpadDebugSession<'data> {
    /// Returns an iterator over all functions in this debug file.
    pub fn functions(&self) -> BreakpadFunctionIterator<'_> {
        BreakpadFunctionIterator::new(&self.file_map, self.lines.clone())
    }

    /// Returns an iterator over all source files in this debug file.
    pub fn files(&self) -> BreakpadFileIterator<'_> {
        BreakpadFileIterator {
            files: self.file_map.values(),
        }
    }

    /// Looks up a file's source contents by its full canonicalized path.
    ///
    /// The given path must be canonicalized.
    pub fn source_by_path(&self, _path: &str) -> Result<Option<Cow<'_, str>>, BreakpadError> {
        Ok(None)
    }
}

impl<'data, 'session> DebugSession<'session> for BreakpadDebugSession<'data> {
    type Error = BreakpadError;
    type FunctionIterator = BreakpadFunctionIterator<'session>;
    type FileIterator = BreakpadFileIterator<'session>;

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

/// An iterator over source files in a Breakpad object.
pub struct BreakpadFileIterator<'s> {
    files: std::collections::btree_map::Values<'s, u64, &'s str>,
}

impl<'s> Iterator for BreakpadFileIterator<'s> {
    type Item = Result<FileEntry<'s>, BreakpadError>;

    fn next(&mut self) -> Option<Self::Item> {
        let path = self.files.next()?;
        Some(Ok(FileEntry {
            compilation_dir: &[],
            info: FileInfo::from_path(path.as_bytes()),
        }))
    }
}

/// An iterator over functions in a Breakpad object.
pub struct BreakpadFunctionIterator<'s> {
    file_map: &'s BreakpadFileMap<'s>,
    next_line: Option<&'s [u8]>,
    inline_origin_map: BreakpadInlineOriginMap<'s>,
    lines: Lines<'s>,
}

impl<'s> BreakpadFunctionIterator<'s> {
    fn new(file_map: &'s BreakpadFileMap<'s>, mut lines: Lines<'s>) -> Self {
        let next_line = lines.next();
        Self {
            file_map,
            next_line,
            inline_origin_map: Default::default(),
            lines,
        }
    }
}

impl<'s> Iterator for BreakpadFunctionIterator<'s> {
    type Item = Result<Function<'s>, BreakpadError>;

    fn next(&mut self) -> Option<Self::Item> {
        // Advance to the next FUNC line.
        let line = loop {
            let line = self.next_line.take()?;
            if line.starts_with(b"FUNC ") {
                break line;
            }

            // Fast path: FUNC records are always before stack records. Once we encounter the
            // first stack record, we can therefore exit.
            if line.starts_with(b"STACK ") {
                return None;
            }

            if line.starts_with(b"INLINE_ORIGIN ") {
                let inline_origin_record = match BreakpadInlineOriginRecord::parse(line) {
                    Ok(record) => record,
                    Err(e) => return Some(Err(e)),
                };
                self.inline_origin_map
                    .insert(inline_origin_record.id, inline_origin_record.name);
            }

            self.next_line = self.lines.next();
        };

        let fun_record = match BreakpadFuncRecord::parse(line, Lines::new(&[])) {
            Ok(record) => record,
            Err(e) => return Some(Err(e)),
        };

        let mut builder = FunctionBuilder::new(
            Name::new(fun_record.name, NameMangling::Unmangled, Language::Unknown),
            b"",
            fun_record.address,
            fun_record.size,
        );

        for line in self.lines.by_ref() {
            // Stop parsing LINE records once other expected records are encountered.
            if line.starts_with(b"FUNC ")
                || line.starts_with(b"PUBLIC ")
                || line.starts_with(b"STACK ")
            {
                self.next_line = Some(line);
                break;
            }

            if line.starts_with(b"INLINE_ORIGIN ") {
                let inline_origin_record = match BreakpadInlineOriginRecord::parse(line) {
                    Ok(record) => record,
                    Err(e) => return Some(Err(e)),
                };
                self.inline_origin_map
                    .insert(inline_origin_record.id, inline_origin_record.name);
                continue;
            }

            if line.starts_with(b"INLINE ") {
                let inline_record = match BreakpadInlineRecord::parse(line) {
                    Ok(record) => record,
                    Err(e) => return Some(Err(e)),
                };

                let name = self
                    .inline_origin_map
                    .get(&inline_record.origin_id)
                    .cloned()
                    .unwrap_or_default();

                for address_range in &inline_record.address_ranges {
                    builder.add_inlinee(
                        inline_record.inline_depth as u32,
                        Name::new(name, NameMangling::Unmangled, Language::Unknown),
                        address_range.address,
                        address_range.size,
                        FileInfo::from_path(
                            self.file_map
                                .get(&inline_record.call_site_file_id)
                                .cloned()
                                .unwrap_or_default()
                                .as_bytes(),
                        ),
                        inline_record.call_site_line,
                    );
                }
                continue;
            }

            // There might be empty lines throughout the file (or at the end). This is the only
            // iterator that cannot rely on a record identifier, so we have to explicitly skip empty
            // lines.
            if line.is_empty() {
                continue;
            }

            let line_record = match BreakpadLineRecord::parse(line) {
                Ok(line_record) => line_record,
                Err(e) => return Some(Err(e)),
            };

            // Skip line records for empty ranges. These do not carry any information.
            if line_record.size == 0 {
                continue;
            }

            let filename = line_record.filename(self.file_map).unwrap_or_default();

            builder.add_leaf_line(
                line_record.address,
                Some(line_record.size),
                FileInfo::from_path(filename.as_bytes()),
                line_record.line,
            );
        }

        Some(Ok(builder.finish()))
    }
}

impl std::iter::FusedIterator for BreakpadFunctionIterator<'_> {}

mod parsing {
    use nom::branch::alt;
    use nom::bytes::complete::take_while;
    use nom::character::complete::{char, hex_digit1, multispace1};
    use nom::combinator::{cond, eof, map, rest};
    use nom::multi::many1;
    use nom::sequence::{pair, tuple};
    use nom::{IResult, Parser};
    use nom_supreme::error::ErrorTree;
    use nom_supreme::final_parser::{Location, RecreateContext};
    use nom_supreme::parser_ext::ParserExt;
    use nom_supreme::tag::complete::tag;

    use super::*;

    type ParseResult<'a, T> = IResult<&'a str, T, ErrorTree<&'a str>>;
    pub type ParseBreakpadError = ErrorTree<ErrorLine>;

    /// A line with a 1-based column position, used for displaying errors.
    ///
    /// With the default formatter, this prints the line followed by the column number.
    /// With the alternate formatter (using `:#`), it prints the line and a caret
    /// pointing at the column position.
    ///
    /// # Example
    /// ```ignore
    /// use symbolic_debuginfo::breakpad::parsing::ErrorLine;
    ///
    /// let error_line = ErrorLine {
    ///     line: "This line cnotains a typo.".to_string(),
    ///     column: 12,
    /// };
    ///
    /// // "This line cnotains a typo.", column 12
    /// println!("{}", error_line);
    ///
    /// // "This line cnotains a typo."
    /// //             ^
    /// println!("{:#}", error_line);
    /// ```
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct ErrorLine {
        /// A line of text containing an error.
        pub line: String,

        /// The position of the error, 1-based.
        pub column: usize,
    }

    impl<'a> RecreateContext<&'a str> for ErrorLine {
        fn recreate_context(original_input: &'a str, tail: &'a str) -> Self {
            let Location { column, .. } = Location::recreate_context(original_input, tail);
            Self {
                line: original_input.to_string(),
                column,
            }
        }
    }

    impl fmt::Display for ErrorLine {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            if f.alternate() {
                writeln!(f)?;
            }

            write!(f, "\"{}\"", self.line)?;

            if f.alternate() {
                writeln!(f, "\n{:>width$}", "^", width = self.column + 1)?;
            } else {
                write!(f, ", column {}", self.column)?;
            }

            Ok(())
        }
    }

    /// Parse a sequence of decimal digits as a number of the given type.
    macro_rules! num_dec {
        ($ty:ty) => {
            nom::character::complete::digit1.map_res(|s: &str| s.parse::<$ty>())
        };
    }

    /// Parse a sequence of hexadecimal digits as a number of the given type.
    macro_rules! num_hex {
        ($ty:ty) => {
            nom::character::complete::hex_digit1.map_res(|n| <$ty>::from_str_radix(n, 16))
        };
    }

    /// Parse a sequence of non-whitespace characters.
    fn non_whitespace(input: &str) -> ParseResult<&str> {
        take_while(|c: char| !c.is_whitespace())(input)
    }

    /// Parse to the end of input and return the resulting string.
    ///
    /// If there is no input, return [`UNKNOWN_NAME`] instead.
    fn name(input: &str) -> ParseResult<&str> {
        rest.map(|name: &str| if name.is_empty() { UNKNOWN_NAME } else { name })
            .parse(input)
    }

    /// Attempt to parse the character `m` followed by one or more spaces.
    ///
    /// Returns true if the parse was successful.
    fn multiple(input: &str) -> ParseResult<bool> {
        let (mut input, multiple) = char('m').opt().parse(input)?;
        let multiple = multiple.is_some();
        if multiple {
            input = multispace1(input)?.0;
        }
        Ok((input, multiple))
    }

    /// Parse a line number as a signed decimal number and return `max(0, n)`.
    fn line_num(input: &str) -> ParseResult<u64> {
        pair(char('-').opt(), num_dec!(u64))
            .map(|(sign, num)| if sign.is_some() { 0 } else { num })
            .parse(input)
    }

    /// Parse a [`BreakpadStackWinRecordType`].
    fn stack_win_record_type(input: &str) -> ParseResult<BreakpadStackWinRecordType> {
        alt((
            char('0').value(BreakpadStackWinRecordType::Fpo),
            char('1').value(BreakpadStackWinRecordType::Trap),
            char('2').value(BreakpadStackWinRecordType::Tss),
            char('3').value(BreakpadStackWinRecordType::Standard),
            char('4').value(BreakpadStackWinRecordType::FrameData),
            non_whitespace.value(BreakpadStackWinRecordType::Unknown),
        ))(input)
    }

    /// Parse a [`BreakpadModuleRecord`].
    ///
    /// A module record has the form `MODULE <os> <arch> <id>( <name>)?`.
    fn module_record(input: &str) -> ParseResult<BreakpadModuleRecord> {
        let (input, _) = tag("MODULE")
            .terminated(multispace1)
            .context("module record prefix")
            .parse(input)?;
        let (input, (os, arch, id, name)) = tuple((
            non_whitespace.terminated(multispace1).context("os"),
            non_whitespace.terminated(multispace1).context("arch"),
            hex_digit1
                .terminated(multispace1.or(eof))
                .context("module id"),
            name.context("module name"),
        ))
        .cut()
        .context("module record body")
        .parse(input)?;

        Ok((input, BreakpadModuleRecord { os, arch, id, name }))
    }

    /// Parse a [`BreakpadModuleRecord`].
    ///
    /// A module record has the form `MODULE <os> <arch> <id>( <name>)?`.
    /// This will fail if there is any input left over after the record.
    pub fn module_record_final(input: &str) -> Result<BreakpadModuleRecord, ErrorTree<ErrorLine>> {
        nom_supreme::final_parser::final_parser(module_record)(input)
    }

    /// Parse the `CodeId` variant of a [`BreakpadInfoRecord`].
    ///
    /// A `CodeId` record has the form `CODE_ID <code_id>( <code_file>)?`.
    fn info_code_id_record(input: &str) -> ParseResult<BreakpadInfoRecord> {
        let (input, _) = tag("CODE_ID")
            .terminated(multispace1)
            .context("info code_id record prefix")
            .parse(input)?;

        let (input, (code_id, code_file)) = pair(
            hex_digit1
                .terminated(multispace1.or(eof))
                .context("code id"),
            name.context("file name"),
        )
        .cut()
        .context("info code_id record body")
        .parse(input)?;

        Ok((input, BreakpadInfoRecord::CodeId { code_id, code_file }))
    }

    /// Parse the `Other` variant of a [`BreakpadInfoRecord`].
    ///
    /// An `Other` record has the form `<scope>( <info>)?`.
    fn info_other_record(input: &str) -> ParseResult<BreakpadInfoRecord> {
        let (input, (scope, info)) = pair(
            non_whitespace
                .terminated(multispace1.or(eof))
                .context("info scope"),
            rest,
        )
        .cut()
        .context("info other record body")
        .parse(input)?;

        Ok((input, BreakpadInfoRecord::Other { scope, info }))
    }

    /// Parse a [`BreakpadInfoRecord`].
    ///
    /// An INFO record has the form `INFO (<code_id_record> | <other_record>)`.
    fn info_record(input: &str) -> ParseResult<BreakpadInfoRecord> {
        let (input, _) = tag("INFO")
            .terminated(multispace1)
            .context("info record prefix")
            .parse(input)?;

        info_code_id_record
            .or(info_other_record)
            .cut()
            .context("info record body")
            .parse(input)
    }

    /// Parse a [`BreakpadInfoRecord`].
    ///
    /// An INFO record has the form `INFO (<code_id_record> | <other_record>)`.
    /// This will fail if there is any input left over after the record.
    pub fn info_record_final(input: &str) -> Result<BreakpadInfoRecord, ErrorTree<ErrorLine>> {
        nom_supreme::final_parser::final_parser(info_record)(input)
    }

    /// Parse a [`BreakpadFileRecord`].
    ///
    /// A FILE record has the form `FILE <id>( <name>)?`.
    fn file_record(input: &str) -> ParseResult<BreakpadFileRecord> {
        let (input, _) = tag("FILE")
            .terminated(multispace1)
            .context("file record prefix")
            .parse(input)?;

        let (input, (id, name)) = pair(
            num_dec!(u64)
                .terminated(multispace1.or(eof))
                .context("file id"),
            rest.context("file name"),
        )
        .cut()
        .context("file record body")
        .parse(input)?;

        Ok((input, BreakpadFileRecord { id, name }))
    }

    /// Parse a [`BreakpadFileRecord`].
    ///
    /// A FILE record has the form `FILE <id>( <name>)?`.
    /// This will fail if there is any input left over after the record.
    pub fn file_record_final(input: &str) -> Result<BreakpadFileRecord, ErrorTree<ErrorLine>> {
        nom_supreme::final_parser::final_parser(file_record)(input)
    }

    /// Parse a [`BreakpadInlineOriginRecord`].
    ///
    /// An INLINE_ORIGIN record has the form `INLINE_ORIGIN <id> <name>`.
    fn inline_origin_record(input: &str) -> ParseResult<BreakpadInlineOriginRecord> {
        let (input, _) = tag("INLINE_ORIGIN")
            .terminated(multispace1)
            .context("inline origin record prefix")
            .parse(input)?;

        let (input, (id, name)) = pair(
            num_dec!(u64)
                .terminated(multispace1)
                .context("inline origin id"),
            rest.context("inline origin name"),
        )
        .cut()
        .context("inline origin record body")
        .parse(input)?;

        Ok((input, BreakpadInlineOriginRecord { id, name }))
    }

    /// Parse a [`BreakpadInlineOriginRecord`].
    ///
    /// An INLINE_ORIGIN record has the form `INLINE_ORIGIN <id> <name>`.
    /// This will fail if there is any input left over after the record.
    pub fn inline_origin_record_final(
        input: &str,
    ) -> Result<BreakpadInlineOriginRecord, ErrorTree<ErrorLine>> {
        nom_supreme::final_parser::final_parser(inline_origin_record)(input)
    }

    /// Parse a [`BreakpadPublicRecord`].
    ///
    /// A PUBLIC record has the form `PUBLIC (m )? <address> <parameter_size> ( <name>)?`.
    fn public_record(input: &str) -> ParseResult<BreakpadPublicRecord> {
        let (input, _) = tag("PUBLIC")
            .terminated(multispace1)
            .context("public record prefix")
            .parse(input)?;

        let (input, (multiple, address, parameter_size, name)) = tuple((
            multiple.context("multiple flag"),
            num_hex!(u64).terminated(multispace1).context("address"),
            num_hex!(u64)
                .terminated(multispace1.or(eof))
                .context("param size"),
            name.context("symbol name"),
        ))
        .cut()
        .context("public record body")
        .parse(input)?;

        Ok((
            input,
            BreakpadPublicRecord {
                multiple,
                address,
                parameter_size,
                name,
            },
        ))
    }

    /// Parse a [`BreakpadPublicRecord`].
    ///
    /// A PUBLIC record has the form `PUBLIC (m )? <address> <parameter_size> ( <name>)?`.
    /// This will fail if there is any input left over after the record.
    pub fn public_record_final(input: &str) -> Result<BreakpadPublicRecord, ErrorTree<ErrorLine>> {
        nom_supreme::final_parser::final_parser(public_record)(input)
    }

    /// Parse a [`BreakpadFuncRecord`].
    ///
    /// A FUNC record has the form `FUNC (m )? <address> <size> <parameter_size> ( <name>)?`.
    fn func_record(input: &str) -> ParseResult<BreakpadFuncRecord> {
        let (input, _) = tag("FUNC")
            .terminated(multispace1)
            .context("func record prefix")
            .parse(input)?;

        let (input, (multiple, address, size, parameter_size, name)) = tuple((
            multiple.context("multiple flag"),
            num_hex!(u64).terminated(multispace1).context("address"),
            num_hex!(u64).terminated(multispace1).context("size"),
            num_hex!(u64)
                .terminated(multispace1.or(eof))
                .context("param size"),
            name.context("symbol name"),
        ))
        .cut()
        .context("func record body")
        .parse(input)?;

        Ok((
            input,
            BreakpadFuncRecord {
                multiple,
                address,
                size,
                parameter_size,
                name,
                lines: Lines::default(),
            },
        ))
    }

    /// Parse a [`BreakpadFuncRecord`].
    ///
    /// A FUNC record has the form `FUNC (m )? <address> <size> <parameter_size> ( <name>)?`.
    /// This will fail if there is any input left over after the record.
    pub fn func_record_final(input: &str) -> Result<BreakpadFuncRecord, ErrorTree<ErrorLine>> {
        nom_supreme::final_parser::final_parser(func_record)(input)
    }

    /// Parse a [`BreakpadLineRecord`].
    ///
    /// A LINE record has the form `<address> <size> <line> <file_id>`.
    fn line_record(input: &str) -> ParseResult<BreakpadLineRecord> {
        let (input, (address, size, line, file_id)) = tuple((
            num_hex!(u64).terminated(multispace1).context("address"),
            num_hex!(u64).terminated(multispace1).context("size"),
            line_num.terminated(multispace1).context("line number"),
            num_dec!(u64).context("file id"),
        ))
        .context("line record")
        .parse(input)?;

        Ok((
            input,
            BreakpadLineRecord {
                address,
                size,
                line,
                file_id,
            },
        ))
    }

    /// Parse a [`BreakpadLineRecord`].
    ///
    /// A LINE record has the form `<address> <size> <line> <file_id>`.
    /// This will fail if there is any input left over after the record.
    pub fn line_record_final(input: &str) -> Result<BreakpadLineRecord, ErrorTree<ErrorLine>> {
        nom_supreme::final_parser::final_parser(line_record)(input)
    }

    /// Parse a [`BreakpadInlineRecord`].
    ///
    /// An INLINE record has the form `INLINE <inline_nest_level> <call_site_line> <call_site_file_id> <origin_id> [<address> <size>]+`.
    fn inline_record(input: &str) -> ParseResult<BreakpadInlineRecord> {
        let (input, _) = tag("INLINE")
            .terminated(multispace1)
            .context("inline record prefix")
            .parse(input)?;

        let (input, (inline_depth, call_site_line, call_site_file_id, origin_id)) = tuple((
            num_dec!(u64)
                .terminated(multispace1)
                .context("inline_nest_level"),
            num_dec!(u64)
                .terminated(multispace1)
                .context("call_site_line"),
            num_dec!(u64)
                .terminated(multispace1)
                .context("call_site_file_id"),
            num_dec!(u64).terminated(multispace1).context("origin_id"),
        ))
        .cut()
        .context("func record body")
        .parse(input)?;

        let (input, address_ranges) = many1(map(
            pair(
                num_hex!(u64).terminated(multispace1).context("address"),
                num_hex!(u64)
                    .terminated(multispace1.or(eof))
                    .context("size"),
            ),
            |(address, size)| BreakpadInlineAddressRange { address, size },
        ))
        .cut()
        .context("inline record body")
        .parse(input)?;

        Ok((
            input,
            BreakpadInlineRecord {
                inline_depth,
                call_site_line,
                call_site_file_id,
                origin_id,
                address_ranges,
            },
        ))
    }

    /// Parse a [`BreakpadInlineRecord`].
    ///
    /// An INLINE record has the form `INLINE <inline_nest_level> <call_site_line> <call_site_file_id> <origin_id> [<address> <size>]+`.
    /// This will fail if there is any input left over after the record.
    pub fn inline_record_final(input: &str) -> Result<BreakpadInlineRecord, ErrorTree<ErrorLine>> {
        nom_supreme::final_parser::final_parser(inline_record)(input)
    }

    /// Parse a [`BreakpadStackCfiDeltaRecord`].
    ///
    /// A STACK CFI Delta record has the form `STACK CFI <address> <rules>`.
    fn stack_cfi_delta_record(input: &str) -> ParseResult<BreakpadStackCfiDeltaRecord> {
        let (input, _) = tag("STACK CFI")
            .terminated(multispace1)
            .context("stack cfi prefix")
            .parse(input)?;

        let (input, (address, rules)) = pair(
            num_hex!(u64).terminated(multispace1).context("address"),
            rest.context("rules"),
        )
        .cut()
        .context("stack cfi delta record body")
        .parse(input)?;

        Ok((input, BreakpadStackCfiDeltaRecord { address, rules }))
    }

    /// Parse a [`BreakpadStackCfiDeltaRecord`].
    ///
    /// A STACK CFI Delta record has the form `STACK CFI <address> <rules>`.
    /// This will fail if there is any input left over after the record.
    pub fn stack_cfi_delta_record_final(
        input: &str,
    ) -> Result<BreakpadStackCfiDeltaRecord, ErrorTree<ErrorLine>> {
        nom_supreme::final_parser::final_parser(stack_cfi_delta_record)(input)
    }

    /// Parse a [`BreakpadStackCfiRecord`].
    ///
    /// A STACK CFI INIT record has the form `STACK CFI INIT <address> <size> <init_rules>`.
    fn stack_cfi_record(input: &str) -> ParseResult<BreakpadStackCfiRecord> {
        let (input, _) = tag("STACK CFI INIT")
            .terminated(multispace1)
            .context("stack cfi init  prefix")
            .parse(input)?;

        let (input, (start, size, init_rules)) = tuple((
            num_hex!(u64).terminated(multispace1).context("start"),
            num_hex!(u64).terminated(multispace1).context("size"),
            rest.context("rules"),
        ))
        .cut()
        .context("stack cfi record body")
        .parse(input)?;

        Ok((
            input,
            BreakpadStackCfiRecord {
                start,
                size,
                init_rules,
                deltas: Lines::default(),
            },
        ))
    }

    /// Parse a [`BreakpadStackCfiRecord`].
    ///
    /// A STACK CFI INIT record has the form `STACK CFI INIT <address> <size> <init_rules>`.
    /// This will fail if there is any input left over after the record.
    pub fn stack_cfi_record_final(
        input: &str,
    ) -> Result<BreakpadStackCfiRecord, ErrorTree<ErrorLine>> {
        nom_supreme::final_parser::final_parser(stack_cfi_record)(input)
    }

    /// Parse a [`BreakpadStackWinRecord`].
    ///
    /// A STACK WIN record has the form
    /// `STACK WIN <ty> <code_start> <code_size> <prolog_size> <epilog_size> <params_size> <saved_regs_size> <locals_size> <max_stack_size> <has_program_string> (<program_string> | <uses_base_pointer>)`.
    fn stack_win_record(input: &str) -> ParseResult<BreakpadStackWinRecord> {
        let (input, _) = tag("STACK WIN")
            .terminated(multispace1)
            .context("stack win prefix")
            .parse(input)?;

        let (
            input,
            (
                ty,
                code_start,
                code_size,
                prolog_size,
                epilog_size,
                params_size,
                saved_regs_size,
                locals_size,
                max_stack_size,
                has_program_string,
            ),
        ) = tuple((
            stack_win_record_type
                .terminated(multispace1)
                .context("record type"),
            num_hex!(u32).terminated(multispace1).context("code start"),
            num_hex!(u32).terminated(multispace1).context("code size"),
            num_hex!(u16).terminated(multispace1).context("prolog size"),
            num_hex!(u16).terminated(multispace1).context("epilog size"),
            num_hex!(u32).terminated(multispace1).context("params size"),
            num_hex!(u16)
                .terminated(multispace1)
                .context("saved regs size"),
            num_hex!(u32).terminated(multispace1).context("locals size"),
            num_hex!(u32)
                .terminated(multispace1)
                .context("max stack size"),
            non_whitespace
                .map(|s| s != "0")
                .terminated(multispace1)
                .context("has_program_string"),
        ))
        .cut()
        .context("stack win record body")
        .parse(input)?;

        let (input, program_string) =
            cond(has_program_string, rest.context("program string"))(input)?;
        let (input, uses_base_pointer) =
            cond(!has_program_string, non_whitespace.map(|s| s != "0"))
                .map(|o| o.unwrap_or(false))
                .parse(input)?;

        Ok((
            input,
            BreakpadStackWinRecord {
                ty,
                code_start,
                code_size,
                prolog_size,
                epilog_size,
                params_size,
                saved_regs_size,
                locals_size,
                max_stack_size,
                uses_base_pointer,
                program_string,
            },
        ))
    }

    /// Parse a [`BreakpadStackWinRecord`].
    ///
    /// A STACK WIN record has the form
    /// `STACK WIN <ty> <code_start> <code_size> <prolog_size> <epilog_size> <params_size> <saved_regs_size> <locals_size> <max_stack_size> <has_program_string> (<program_string> | <uses_base_pointer>)`.
    /// This will fail if there is any input left over after the record.
    pub fn stack_win_record_final(
        input: &str,
    ) -> Result<BreakpadStackWinRecord, ErrorTree<ErrorLine>> {
        nom_supreme::final_parser::final_parser(stack_win_record)(input)
    }

    /// Parse a [`BreakpadStackRecord`], containing either a [`BreakpadStackCfiRecord`] or a
    /// [`BreakpadStackWinRecord`].
    ///
    /// This will fail if there is any input left over after the record.
    pub fn stack_record_final(input: &str) -> Result<BreakpadStackRecord, ParseBreakpadError> {
        nom_supreme::final_parser::final_parser(alt((
            stack_cfi_record.map(BreakpadStackRecord::Cfi),
            stack_win_record.map(BreakpadStackRecord::Win),
        )))(input)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_module_record() -> Result<(), BreakpadError> {
        let string = b"MODULE Linux x86_64 492E2DD23CC306CA9C494EEF1533A3810 crash";
        let record = BreakpadModuleRecord::parse(&*string)?;

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadModuleRecord {
       ⋮    os: "Linux",
       ⋮    arch: "x86_64",
       ⋮    id: "492E2DD23CC306CA9C494EEF1533A3810",
       ⋮    name: "crash",
       ⋮}
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_module_record_short_id() -> Result<(), BreakpadError> {
        // NB: This id is one character short, missing the age. DebugId can handle this, however.
        let string = b"MODULE Linux x86_64 6216C672A8D33EC9CF4A1BAB8B29D00E libdispatch.so";
        let record = BreakpadModuleRecord::parse(string)?;

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadModuleRecord {
       ⋮    os: "Linux",
       ⋮    arch: "x86_64",
       ⋮    id: "6216C672A8D33EC9CF4A1BAB8B29D00E",
       ⋮    name: "libdispatch.so",
       ⋮}
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_file_record() -> Result<(), BreakpadError> {
        let string = b"FILE 37 /usr/include/libkern/i386/_OSByteOrder.h";
        let record = BreakpadFileRecord::parse(string)?;

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadFileRecord {
       ⋮    id: 37,
       ⋮    name: "/usr/include/libkern/i386/_OSByteOrder.h",
       ⋮}
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_file_record_space() -> Result<(), BreakpadError> {
        let string = b"FILE 38 /usr/local/src/filename with spaces.c";
        let record = BreakpadFileRecord::parse(string)?;

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadFileRecord {
       ⋮    id: 38,
       ⋮    name: "/usr/local/src/filename with spaces.c",
       ⋮}
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_inline_origin_record() -> Result<(), BreakpadError> {
        let string = b"INLINE_ORIGIN 3529 LZ4F_initStream";
        let record = BreakpadInlineOriginRecord::parse(string)?;

        insta::assert_debug_snapshot!(record, @r###"
        BreakpadInlineOriginRecord {
            id: 3529,
            name: "LZ4F_initStream",
        }
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_inline_origin_record_space() -> Result<(), BreakpadError> {
        let string =
            b"INLINE_ORIGIN 3576 unsigned int mozilla::AddToHash<char, 0>(unsigned int, char)";
        let record = BreakpadInlineOriginRecord::parse(string)?;

        insta::assert_debug_snapshot!(record, @r###"
        BreakpadInlineOriginRecord {
            id: 3576,
            name: "unsigned int mozilla::AddToHash<char, 0>(unsigned int, char)",
        }
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_func_record() -> Result<(), BreakpadError> {
        // Lines will be tested separately
        let string = b"FUNC 1730 1a 0 <name omitted>";
        let record = BreakpadFuncRecord::parse(string, Lines::default())?;

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadFuncRecord {
       ⋮    multiple: false,
       ⋮    address: 5936,
       ⋮    size: 26,
       ⋮    parameter_size: 0,
       ⋮    name: "<name omitted>",
       ⋮}
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_func_record_multiple() -> Result<(), BreakpadError> {
        let string = b"FUNC m 1730 1a 0 <name omitted>";
        let record = BreakpadFuncRecord::parse(string, Lines::default())?;

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadFuncRecord {
       ⋮    multiple: true,
       ⋮    address: 5936,
       ⋮    size: 26,
       ⋮    parameter_size: 0,
       ⋮    name: "<name omitted>",
       ⋮}
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_func_record_no_name() -> Result<(), BreakpadError> {
        let string = b"FUNC 0 f 0";
        let record = BreakpadFuncRecord::parse(string, Lines::default())?;

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadFuncRecord {
       ⋮    multiple: false,
       ⋮    address: 0,
       ⋮    size: 15,
       ⋮    parameter_size: 0,
       ⋮    name: "<unknown>",
       ⋮}
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_line_record() -> Result<(), BreakpadError> {
        let string = b"1730 6 93 20";
        let record = BreakpadLineRecord::parse(string)?;

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadLineRecord {
       ⋮    address: 5936,
       ⋮    size: 6,
       ⋮    line: 93,
       ⋮    file_id: 20,
       ⋮}
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_line_record_negative_line() -> Result<(), BreakpadError> {
        let string = b"e0fd10 5 -376 2225";
        let record = BreakpadLineRecord::parse(string)?;

        insta::assert_debug_snapshot!(record, @r###"
        BreakpadLineRecord {
            address: 14744848,
            size: 5,
            line: 0,
            file_id: 2225,
        }
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_line_record_whitespace() -> Result<(), BreakpadError> {
        let string = b"    1000 1c 2972 2
";
        let record = BreakpadLineRecord::parse(string)?;

        insta::assert_debug_snapshot!(
            record, @r###"
        BreakpadLineRecord {
            address: 4096,
            size: 28,
            line: 2972,
            file_id: 2,
        }
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_public_record() -> Result<(), BreakpadError> {
        let string = b"PUBLIC 5180 0 __clang_call_terminate";
        let record = BreakpadPublicRecord::parse(string)?;

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadPublicRecord {
       ⋮    multiple: false,
       ⋮    address: 20864,
       ⋮    parameter_size: 0,
       ⋮    name: "__clang_call_terminate",
       ⋮}
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_public_record_multiple() -> Result<(), BreakpadError> {
        let string = b"PUBLIC m 5180 0 __clang_call_terminate";
        let record = BreakpadPublicRecord::parse(string)?;

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadPublicRecord {
       ⋮    multiple: true,
       ⋮    address: 20864,
       ⋮    parameter_size: 0,
       ⋮    name: "__clang_call_terminate",
       ⋮}
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_public_record_no_name() -> Result<(), BreakpadError> {
        let string = b"PUBLIC 5180 0";
        let record = BreakpadPublicRecord::parse(string)?;

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadPublicRecord {
       ⋮    multiple: false,
       ⋮    address: 20864,
       ⋮    parameter_size: 0,
       ⋮    name: "<unknown>",
       ⋮}
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_inline_record() -> Result<(), BreakpadError> {
        let string = b"INLINE 0 3082 52 1410 49200 10";
        let record = BreakpadInlineRecord::parse(string)?;

        insta::assert_debug_snapshot!(record, @r###"
        BreakpadInlineRecord {
            inline_depth: 0,
            call_site_line: 3082,
            call_site_file_id: 52,
            origin_id: 1410,
            address_ranges: [
                BreakpadInlineAddressRange {
                    address: 299520,
                    size: 16,
                },
            ],
        }
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_inline_record_multiple() -> Result<(), BreakpadError> {
        let string = b"INLINE 6 642 8 207 8b110 18 8b154 18";
        let record = BreakpadInlineRecord::parse(string)?;

        insta::assert_debug_snapshot!(record, @r###"
        BreakpadInlineRecord {
            inline_depth: 6,
            call_site_line: 642,
            call_site_file_id: 8,
            origin_id: 207,
            address_ranges: [
                BreakpadInlineAddressRange {
                    address: 569616,
                    size: 24,
                },
                BreakpadInlineAddressRange {
                    address: 569684,
                    size: 24,
                },
            ],
        }
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_inline_record_err_missing_address_range() {
        let string = b"INLINE 6 642 8 207";
        let record = BreakpadInlineRecord::parse(string);
        assert!(record.is_err());
    }

    #[test]
    fn test_parse_stack_cfi_init_record() -> Result<(), BreakpadError> {
        let string = b"STACK CFI INIT 1880 2d .cfa: $rsp 8 + .ra: .cfa -8 + ^";
        let record = BreakpadStackRecord::parse(string)?;

        insta::assert_debug_snapshot!(record, @r###"
        Cfi(
            BreakpadStackCfiRecord {
                start: 6272,
                size: 45,
                init_rules: ".cfa: $rsp 8 + .ra: .cfa -8 + ^",
                deltas: Lines(
                    LineOffsets {
                        data: [],
                        finished: true,
                        index: 0,
                    },
                ),
            },
        )
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_stack_win_record() -> Result<(), BreakpadError> {
        let string =
            b"STACK WIN 4 371a c 0 0 0 0 0 0 1 $T0 .raSearch = $eip $T0 ^ = $esp $T0 4 + =";
        let record = BreakpadStackRecord::parse(string)?;

        insta::assert_debug_snapshot!(record, @r###"
        Win(
            BreakpadStackWinRecord {
                ty: FrameData,
                code_start: 14106,
                code_size: 12,
                prolog_size: 0,
                epilog_size: 0,
                params_size: 0,
                saved_regs_size: 0,
                locals_size: 0,
                max_stack_size: 0,
                uses_base_pointer: false,
                program_string: Some(
                    "$T0 .raSearch = $eip $T0 ^ = $esp $T0 4 + =",
                ),
            },
        )
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_stack_win_record_type_3() -> Result<(), BreakpadError> {
        let string = b"STACK WIN 3 8a10b ec b 0 c c 4 0 0 1";
        let record = BreakpadStackWinRecord::parse(string)?;

        insta::assert_debug_snapshot!(record, @r###"
        BreakpadStackWinRecord {
            ty: Standard,
            code_start: 565515,
            code_size: 236,
            prolog_size: 11,
            epilog_size: 0,
            params_size: 12,
            saved_regs_size: 12,
            locals_size: 4,
            max_stack_size: 0,
            uses_base_pointer: true,
            program_string: None,
        }
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_stack_win_whitespace() -> Result<(), BreakpadError> {
        let string =
            b"     STACK WIN 4 371a c 0 0 0 0 0 0 1 $T0 .raSearch = $eip $T0 ^ = $esp $T0 4 + =
                ";
        let record = BreakpadStackRecord::parse(string)?;

        insta::assert_debug_snapshot!(record, @r###"
        Win(
            BreakpadStackWinRecord {
                ty: FrameData,
                code_start: 14106,
                code_size: 12,
                prolog_size: 0,
                epilog_size: 0,
                params_size: 0,
                saved_regs_size: 0,
                locals_size: 0,
                max_stack_size: 0,
                uses_base_pointer: false,
                program_string: Some(
                    "$T0 .raSearch = $eip $T0 ^ = $esp $T0 4 + =",
                ),
            },
        )
        "###);
        Ok(())
    }

    use similar_asserts::assert_eq;

    #[test]
    fn test_lineoffsets_fused() {
        let data = b"";
        let mut offsets = LineOffsets::new(data);

        offsets.next();
        assert_eq!(None, offsets.next());
        assert_eq!(None, offsets.next());
        assert_eq!(None, offsets.next());
    }

    macro_rules! test_lineoffsets {
        ($name:ident, $data:literal, $( ($index:literal, $line:literal) ),*) => {
            #[test]
            fn $name() {
                let mut offsets = LineOffsets::new($data);

                $(
                    assert_eq!(Some(($index, &$line[..])), offsets.next());
                )*
                assert_eq!(None, offsets.next());
            }
        };
    }

    test_lineoffsets!(test_lineoffsets_empty, b"", (0, b""));
    test_lineoffsets!(test_lineoffsets_oneline, b"hello", (0, b"hello"));
    test_lineoffsets!(
        test_lineoffsets_trailing_n,
        b"hello\n",
        (0, b"hello"),
        (6, b"")
    );
    test_lineoffsets!(
        test_lineoffsets_trailing_rn,
        b"hello\r\n",
        (0, b"hello"),
        (7, b"")
    );
    test_lineoffsets!(
        test_lineoffsets_n,
        b"hello\nworld\nyo",
        (0, b"hello"),
        (6, b"world"),
        (12, b"yo")
    );
    test_lineoffsets!(
        test_lineoffsets_rn,
        b"hello\r\nworld\r\nyo",
        (0, b"hello"),
        (7, b"world"),
        (14, b"yo")
    );
    test_lineoffsets!(
        test_lineoffsets_mixed,
        b"hello\r\nworld\nyo",
        (0, b"hello"),
        (7, b"world"),
        (13, b"yo")
    );
}
