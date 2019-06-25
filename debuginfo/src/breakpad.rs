//! Support for Breakpad ASCII symbols, used by the Breakpad and Crashpad libraries.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::str;

use failure::Fail;
use pest::Parser;

use symbolic_common::{derive_failure, Arch, AsSelf, CodeId, DebugId, Name};

use crate::base::*;
use crate::private::{Lines, Parse};

mod parser {
    use pest_derive::Parser;

    #[derive(Debug, Parser)]
    #[grammar = "breakpad.pest"]
    pub struct BreakpadParser;
}

use self::parser::{BreakpadParser, Rule};

/// Length at which the breakpad header will be capped.
///
/// This is a protection against reading an entire breakpad file at once if the first characters do
/// not contain a valid line break.
const BREAKPAD_HEADER_CAP: usize = 320;

/// Variants of `BreakpadError`.
#[derive(Debug, Fail)]
pub enum BreakpadErrorKind {
    /// The symbol header (`MODULE` record) is missing.
    #[fail(display = "missing breakpad symbol header")]
    InvalidMagic,

    /// A part of the file is not encoded in valid UTF-8.
    #[fail(display = "bad utf-8 sequence")]
    BadEncoding(#[fail(cause)] str::Utf8Error),

    /// A record violates the Breakpad symbol syntax.
    #[fail(display = "{}", _0)]
    BadSyntax(pest::error::Error<Rule>),

    /// Parsing of a record failed.
    #[fail(display = "{}", _0)]
    Parse(&'static str),
}

derive_failure!(
    BreakpadError,
    BreakpadErrorKind,
    doc = "An error when dealing with [`BreakpadObject`](struct.BreakpadObject.html).",
);

impl From<str::Utf8Error> for BreakpadError {
    fn from(error: str::Utf8Error) -> Self {
        BreakpadErrorKind::BadEncoding(error).into()
    }
}

impl From<pest::error::Error<Rule>> for BreakpadError {
    fn from(error: pest::error::Error<Rule>) -> Self {
        BreakpadErrorKind::BadSyntax(error).into()
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
        let parsed = BreakpadParser::parse(Rule::module, string)?.next().unwrap();
        let mut record = BreakpadModuleRecord::default();

        for pair in parsed.into_inner() {
            match pair.as_rule() {
                Rule::os => record.os = pair.as_str(),
                Rule::arch => record.arch = pair.as_str(),
                Rule::debug_id => record.id = pair.as_str(),
                Rule::name => record.name = pair.as_str(),
                _ => unreachable!(),
            }
        }

        Ok(record)
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
        let parsed = BreakpadParser::parse(Rule::info, string)?.next().unwrap();

        for pair in parsed.into_inner() {
            match pair.as_rule() {
                Rule::info_code_id => return Self::code_info_from_pair(pair),
                Rule::info_other => return Self::other_from_pair(pair),
                _ => unreachable!(),
            }
        }

        Err(BreakpadErrorKind::Parse("unknown INFO record").into())
    }

    fn code_info_from_pair(pair: pest::iterators::Pair<'d, Rule>) -> Result<Self, BreakpadError> {
        let mut code_id = "";
        let mut code_file = "";

        for pair in pair.into_inner() {
            match pair.as_rule() {
                Rule::code_id => code_id = pair.as_str(),
                Rule::name => code_file = pair.as_str(),
                _ => unreachable!(),
            }
        }

        Ok(BreakpadInfoRecord::CodeId { code_id, code_file })
    }

    fn other_from_pair(pair: pest::iterators::Pair<'d, Rule>) -> Result<Self, BreakpadError> {
        let mut scope = "";
        let mut info = "";

        for pair in pair.into_inner() {
            match pair.as_rule() {
                Rule::ident => scope = pair.as_str(),
                Rule::text => info = pair.as_str(),
                _ => unreachable!(),
            }
        }

        Ok(BreakpadInfoRecord::Other { scope, info })
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

        while let Some(line) = self.lines.next() {
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
        let parsed = BreakpadParser::parse(Rule::file, string)?.next().unwrap();
        let mut record = BreakpadFileRecord::default();

        for pair in parsed.into_inner() {
            match pair.as_rule() {
                Rule::file_id => {
                    record.id = u64::from_str_radix(pair.as_str(), 10)
                        .map_err(|_| BreakpadErrorKind::Parse("file identifier"))?;
                }
                Rule::name => record.name = pair.as_str(),
                _ => unreachable!(),
            }
        }

        Ok(record)
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

        while let Some(line) = self.lines.next() {
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
        let parsed = BreakpadParser::parse(Rule::public, string)?.next().unwrap();
        let mut record = BreakpadPublicRecord::default();

        for pair in parsed.into_inner() {
            match pair.as_rule() {
                Rule::multiple => record.multiple = true,
                Rule::addr => {
                    record.address = u64::from_str_radix(pair.as_str(), 16)
                        .map_err(|_| BreakpadErrorKind::Parse("symbol address"))?;
                }
                Rule::param_size => {
                    record.parameter_size = u64::from_str_radix(pair.as_str(), 16)
                        .map_err(|_| BreakpadErrorKind::Parse("symbol parameter size"))?;
                }
                Rule::name => record.name = pair.as_str(),
                _ => unreachable!(),
            }
        }

        Ok(record)
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

        while let Some(line) = self.lines.next() {
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
        let parsed = BreakpadParser::parse(Rule::func, string)?.next().unwrap();
        let mut record = BreakpadFuncRecord::default();

        for pair in parsed.into_inner() {
            match pair.as_rule() {
                Rule::multiple => record.multiple = true,
                Rule::addr => {
                    record.address = u64::from_str_radix(pair.as_str(), 16)
                        .map_err(|_| BreakpadErrorKind::Parse("function address"))?;
                }
                Rule::size => {
                    record.size = u64::from_str_radix(pair.as_str(), 16)
                        .map_err(|_| BreakpadErrorKind::Parse("function size"))?;
                }
                Rule::param_size => {
                    record.parameter_size = u64::from_str_radix(pair.as_str(), 16)
                        .map_err(|_| BreakpadErrorKind::Parse("function parameter size"))?;
                }
                Rule::name => record.name = pair.as_str(),
                _ => unreachable!(),
            }
        }

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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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

        while let Some(line) = self.lines.next() {
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
        let parsed = BreakpadParser::parse(Rule::line, string)?.next().unwrap();
        let mut record = BreakpadLineRecord::default();

        for pair in parsed.into_inner() {
            match pair.as_rule() {
                Rule::addr => {
                    record.address = u64::from_str_radix(pair.as_str(), 16)
                        .map_err(|_| BreakpadErrorKind::Parse("line address"))?;
                }
                Rule::size => {
                    record.size = u64::from_str_radix(pair.as_str(), 16)
                        .map_err(|_| BreakpadErrorKind::Parse("line size"))?;
                }
                Rule::line_num => {
                    // NB: Breakpad does not allow negative line numbers and even tests that the
                    // symbol parser rejects such line records. However, negative line numbers have
                    // been observed at least for ELF files, so handle them gracefully.
                    record.line = i32::from_str_radix(pair.as_str(), 10)
                        .map(|line| u64::from(line as u32))
                        .map_err(|_| BreakpadErrorKind::Parse("line number"))?;
                }
                Rule::file_id => {
                    record.file_id = u64::from_str_radix(pair.as_str(), 10)
                        .map_err(|_| BreakpadErrorKind::Parse("file number"))?;
                }
                _ => unreachable!(),
            }
        }

        Ok(record)
    }

    /// Resolves the filename for this record in the file map.
    pub fn filename<'d>(&self, file_map: &BreakpadFileMap<'d>) -> Option<&'d str> {
        file_map.get(&self.file_id).cloned()
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

        while let Some(line) = self.lines.next() {
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

/// A [call frame information record] for platforms other than Windows x86.
///
/// Example: `STACK CFI INIT 804c4b0 40 .cfa: $esp 4 + $eip: .cfa 4 - ^`
///
/// [call frame information record]: https://github.com/google/breakpad/blob/master/docs/symbol_files.md#stack-cfi-records
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BreakpadStackCfiRecord<'d> {
    /// The unwind program rules.
    pub text: &'d str,
}

impl<'d> BreakpadStackCfiRecord<'d> {
    /// Parses a CFI stack record from a single line.
    pub fn parse(data: &'d [u8]) -> Result<Self, BreakpadError> {
        let string = str::from_utf8(data)?;
        let parsed = BreakpadParser::parse(Rule::stack_cfi, string)?
            .next()
            .unwrap();

        Self::from_pair(parsed)
    }

    /// Constructs a stack record directly from a Pest parser pair.
    fn from_pair(pair: pest::iterators::Pair<'d, Rule>) -> Result<Self, BreakpadError> {
        let mut record = BreakpadStackCfiRecord::default();

        for pair in pair.into_inner() {
            match pair.as_rule() {
                Rule::text => record.text = pair.as_str(),
                _ => unreachable!(),
            }
        }

        Ok(record)
    }
}

/// A [Windows stack frame record], used on x86.
///
/// Example: `STACK WIN 4 2170 14 1 0 0 0 0 0 1 $eip 4 + ^ = $esp $ebp 8 + = $ebp $ebp ^ =`
///
/// [Windows stack frame record]: https://github.com/google/breakpad/blob/master/docs/symbol_files.md#stack-win-records
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BreakpadStackWinRecord<'d> {
    /// Variables and the program string.
    pub text: &'d str,
}

impl<'d> BreakpadStackWinRecord<'d> {
    /// Parses a Windows stack record from a single line.
    pub fn parse(data: &'d [u8]) -> Result<Self, BreakpadError> {
        let string = str::from_utf8(data)?;
        let parsed = BreakpadParser::parse(Rule::stack_win, string)?
            .next()
            .unwrap();

        Self::from_pair(parsed)
    }

    // Constructs a stack record directly from a Pest parser pair.
    fn from_pair(pair: pest::iterators::Pair<'d, Rule>) -> Result<Self, BreakpadError> {
        let mut record = BreakpadStackWinRecord::default();

        for pair in pair.into_inner() {
            match pair.as_rule() {
                Rule::text => record.text = pair.as_str(),
                _ => unreachable!(),
            }
        }

        Ok(record)
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
        let parsed = BreakpadParser::parse(Rule::stack, string)?.next().unwrap();
        let pair = parsed.into_inner().next().unwrap();

        Ok(match pair.as_rule() {
            Rule::stack_cfi => BreakpadStackRecord::Cfi(BreakpadStackCfiRecord::from_pair(pair)?),
            Rule::stack_win => BreakpadStackRecord::Win(BreakpadStackWinRecord::from_pair(pair)?),
            _ => unreachable!(),
        })
    }
}

/// An iterator over stack frame records in a Breakpad object.
#[derive(Clone, Debug)]
pub struct BreakpadStackRecords<'d> {
    lines: Lines<'d>,
    finished: bool,
}

impl<'d> Iterator for BreakpadStackRecords<'d> {
    type Item = Result<BreakpadStackRecord<'d>, BreakpadError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        while let Some(line) = self.lines.next() {
            if line.starts_with(b"STACK ") {
                return Some(BreakpadStackRecord::parse(line));
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
pub struct BreakpadObject<'d> {
    id: DebugId,
    arch: Arch,
    module: BreakpadModuleRecord<'d>,
    data: &'d [u8],
}

impl<'d> BreakpadObject<'d> {
    /// Tests whether the buffer could contain a Breakpad object.
    pub fn test(data: &[u8]) -> bool {
        data.starts_with(b"MODULE ")
    }

    /// Tries to parse a Breakpad object from the given slice.
    pub fn parse(data: &'d [u8]) -> Result<Self, BreakpadError> {
        // Ensure that we do not read the entire file at once.
        let header = if data.len() > BREAKPAD_HEADER_CAP {
            match str::from_utf8(&data[..BREAKPAD_HEADER_CAP]) {
                Ok(_) => &data[..BREAKPAD_HEADER_CAP],
                Err(e) => match e.error_len() {
                    None => &data[..e.valid_up_to()],
                    Some(_) => Err(e)?,
                },
            }
        } else {
            data
        };

        let module = BreakpadModuleRecord::parse(header)?;

        Ok(BreakpadObject {
            id: module
                .id
                .parse()
                .map_err(|_| BreakpadErrorKind::Parse("module id"))?,
            arch: module
                .arch
                .parse()
                .map_err(|_| BreakpadErrorKind::Parse("module architecture"))?,
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
        for result in self.info_records() {
            if let Ok(record) = result {
                if let BreakpadInfoRecord::CodeId { code_id, .. } = record {
                    if !code_id.is_empty() {
                        return Some(CodeId::new(code_id.into()));
                    }
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
    pub fn name(&self) -> &'d str {
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
    pub fn symbols(&self) -> BreakpadSymbolIterator<'d> {
        BreakpadSymbolIterator {
            records: self.public_records(),
        }
    }

    /// Returns an ordered map of symbols in the symbol table.
    pub fn symbol_map(&self) -> SymbolMap<'d> {
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
    pub fn debug_session(&self) -> Result<BreakpadDebugSession<'d>, BreakpadError> {
        Ok(BreakpadDebugSession {
            file_map: self.file_map(),
            func_records: self.func_records(),
        })
    }

    /// Determines whether this object contains stack unwinding information.
    pub fn has_unwind_info(&self) -> bool {
        self.stack_records().next().is_some()
    }

    /// Determines whether this object contains embedded source.
    pub fn has_source(&self) -> bool {
        false
    }

    /// Returns an iterator over info records.
    pub fn info_records(&self) -> BreakpadInfoRecords<'d> {
        BreakpadInfoRecords {
            lines: Lines::new(self.data),
            finished: false,
        }
    }

    /// Returns an iterator over file records.
    pub fn file_records(&self) -> BreakpadFileRecords<'d> {
        BreakpadFileRecords {
            lines: Lines::new(self.data),
            finished: false,
        }
    }

    /// Returns a map for file name lookups by id.
    pub fn file_map(&self) -> BreakpadFileMap<'d> {
        self.file_records()
            .filter_map(Result::ok)
            .map(|file| (file.id, file.name))
            .collect()
    }

    /// Returns an iterator over public symbol records.
    pub fn public_records(&self) -> BreakpadPublicRecords<'d> {
        BreakpadPublicRecords {
            lines: Lines::new(self.data),
            finished: false,
        }
    }

    /// Returns an iterator over function records.
    pub fn func_records(&self) -> BreakpadFuncRecords<'d> {
        BreakpadFuncRecords {
            lines: Lines::new(self.data),
            finished: false,
        }
    }

    /// Returns an iterator over stack frame records.
    pub fn stack_records(&self) -> BreakpadStackRecords<'d> {
        BreakpadStackRecords {
            lines: Lines::new(self.data),
            finished: false,
        }
    }

    /// Returns the raw data of the Breakpad file.
    pub fn data(&self) -> &'d [u8] {
        self.data
    }
}

impl fmt::Debug for BreakpadObject<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("BreakpadObject")
            .field("code_id", &self.code_id())
            .field("debug_id", &self.debug_id())
            .field("arch", &self.arch())
            .field("name", &self.name())
            .field("has_symbols", &self.has_symbols())
            .field("has_debug_info", &self.has_debug_info())
            .field("has_unwind_info", &self.has_unwind_info())
            .finish()
    }
}

impl<'slf, 'd: 'slf> AsSelf<'slf> for BreakpadObject<'d> {
    type Ref = BreakpadObject<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        self
    }
}

impl<'d> Parse<'d> for BreakpadObject<'d> {
    type Error = BreakpadError;

    fn test(data: &[u8]) -> bool {
        Self::test(data)
    }

    fn parse(data: &'d [u8]) -> Result<Self, BreakpadError> {
        Self::parse(data)
    }
}

impl<'d> ObjectLike for BreakpadObject<'d> {
    type Error = BreakpadError;
    type Session = BreakpadDebugSession<'d>;

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

    fn symbols(&self) -> DynIterator<'_, Symbol<'_>> {
        Box::new(self.symbols())
    }

    fn symbol_map(&self) -> SymbolMap<'_> {
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

    fn has_source(&self) -> bool {
        self.has_source()
    }
}

/// An iterator over symbols in the Breakpad object.
///
/// Returned by [`BreakpadObject::symbols`](struct.BreakpadObject.html#method.symbols).
pub struct BreakpadSymbolIterator<'d> {
    records: BreakpadPublicRecords<'d>,
}

impl<'d> Iterator for BreakpadSymbolIterator<'d> {
    type Item = Symbol<'d>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(result) = self.records.next() {
            if let Ok(record) = result {
                return Some(Symbol {
                    name: Some(Cow::Borrowed(record.name)),
                    address: record.address,
                    size: 0,
                });
            }
        }

        None
    }
}

/// Debug session for Breakpad objects.
pub struct BreakpadDebugSession<'d> {
    file_map: BreakpadFileMap<'d>,
    func_records: BreakpadFuncRecords<'d>,
}

impl<'d> BreakpadDebugSession<'d> {
    /// Returns an iterator over all functions in this debug file.
    pub fn functions(&mut self) -> BreakpadFunctionIterator<'_> {
        BreakpadFunctionIterator {
            file_map: &self.file_map,
            func_records: self.func_records.clone(),
        }
    }
}

impl<'d> DebugSession for BreakpadDebugSession<'d> {
    type Error = BreakpadError;

    fn functions(&mut self) -> DynIterator<'_, Result<Function<'_>, Self::Error>> {
        Box::new(self.functions())
    }
}

/// An iterator over functions in a Breakpad object.
pub struct BreakpadFunctionIterator<'s> {
    file_map: &'s BreakpadFileMap<'s>,
    func_records: BreakpadFuncRecords<'s>,
}

impl<'s> BreakpadFunctionIterator<'s> {
    fn convert(&self, record: BreakpadFuncRecord<'s>) -> Result<Function<'s>, BreakpadError> {
        let mut lines = Vec::new();
        for line in record.lines() {
            let line = line?;
            let filename = line.filename(&self.file_map).unwrap_or_default();
            let (dir, name) = symbolic_common::split_path(filename);

            lines.push(LineInfo {
                address: line.address,
                file: FileInfo {
                    name: name.as_bytes(),
                    dir: dir.unwrap_or_default().as_bytes(),
                },
                line: line.line,
            });
        }

        Ok(Function {
            address: record.address,
            size: record.size,
            name: Name::from(record.name),
            compilation_dir: &[],
            lines,
            inlinees: Vec::new(),
            inline: false,
        })
    }
}

impl<'s> Iterator for BreakpadFunctionIterator<'s> {
    type Item = Result<Function<'s>, BreakpadError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.func_records.next() {
            Some(Ok(record)) => Some(self.convert(record)),
            Some(Err(error)) => Some(Err(error)),
            None => None,
        }
    }
}

impl std::iter::FusedIterator for BreakpadFunctionIterator<'_> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_module_record() -> Result<(), BreakpadError> {
        let string = b"MODULE Linux x86_64 492E2DD23CC306CA9C494EEF1533A3810 crash";
        let record = BreakpadModuleRecord::parse(&*string)?;

        insta::assert_debug_snapshot_matches!(record, @r###"
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

        insta::assert_debug_snapshot_matches!(record, @r###"
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

        insta::assert_debug_snapshot_matches!(record, @r###"
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

        insta::assert_debug_snapshot_matches!(record, @r###"
       ⋮BreakpadFileRecord {
       ⋮    id: 38,
       ⋮    name: "/usr/local/src/filename with spaces.c",
       ⋮}
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_func_record() -> Result<(), BreakpadError> {
        // Lines will be tested separately
        let string = b"FUNC 1730 1a 0 <name omitted>";
        let record = BreakpadFuncRecord::parse(string, Lines::default())?;

        insta::assert_debug_snapshot_matches!(record, @r###"
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

        insta::assert_debug_snapshot_matches!(record, @r###"
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
    fn test_parse_line_record() -> Result<(), BreakpadError> {
        let string = b"1730 6 93 20";
        let record = BreakpadLineRecord::parse(string)?;

        insta::assert_debug_snapshot_matches!(record, @r###"
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

        insta::assert_debug_snapshot_matches!(record, @r###"
       ⋮BreakpadLineRecord {
       ⋮    address: 14744848,
       ⋮    size: 5,
       ⋮    line: 4294966920,
       ⋮    file_id: 2225,
       ⋮}
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_public_record() -> Result<(), BreakpadError> {
        let string = b"PUBLIC 5180 0 __clang_call_terminate";
        let record = BreakpadPublicRecord::parse(string)?;

        insta::assert_debug_snapshot_matches!(record, @r###"
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

        insta::assert_debug_snapshot_matches!(record, @r###"
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
    fn test_parse_stack_cfi_record() -> Result<(), BreakpadError> {
        let string = b"STACK CFI INIT 1880 2d .cfa: $rsp 8 + .ra: .cfa -8 + ^";
        let record = BreakpadStackRecord::parse(string)?;

        insta::assert_debug_snapshot_matches!(record, @r###"
       ⋮Cfi(
       ⋮    BreakpadStackCfiRecord {
       ⋮        text: "INIT 1880 2d .cfa: $rsp 8 + .ra: .cfa -8 + ^",
       ⋮    },
       ⋮)
        "###);

        Ok(())
    }

    #[test]
    fn test_parse_stack_win_record() -> Result<(), BreakpadError> {
        let string =
            b"STACK WIN 4 371a c 0 0 0 0 0 0 1 $T0 .raSearch = $eip $T0 ^ = $esp $T0 4 + =";
        let record = BreakpadStackRecord::parse(string)?;

        insta::assert_debug_snapshot_matches!(record, @r###"
       ⋮Win(
       ⋮    BreakpadStackWinRecord {
       ⋮        text: "4 371a c 0 0 0 0 0 0 1 $T0 .raSearch = $eip $T0 ^ = $esp $T0 4 + =",
       ⋮    },
       ⋮)
        "###);

        Ok(())
    }
}
