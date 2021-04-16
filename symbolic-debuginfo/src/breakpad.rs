//! Support for Breakpad ASCII symbols, used by the Breakpad and Crashpad libraries.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::str;

use thiserror::Error;

use symbolic_common::{
    Arch, AsSelf, CodeId, DebugId, Language, Name, NameMangling, ParseDebugIdError,
};

use crate::base::*;
use crate::private::{Lines, Parse};

// type Result<A> = std::result::Result<A, BreakpadError>;
type ParseResult<A> = std::result::Result<A, ParseBreakpadError>;

/// Length at which the breakpad header will be capped.
///
/// This is a protection against reading an entire breakpad file at once if the first characters do
/// not contain a valid line break.
const BREAKPAD_HEADER_CAP: usize = 320;

/// Placeholder used for missing function or symbol names.
const UNKNOWN_NAME: &str = "<unknown>";

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParseBreakpadErrorKind {
    Arch,
    Id,
    Line,
    MissingInput,
    NumDec,
    NumHex,
    Os,
    StackWinRecordType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BreakpadRecordType {
    File,
    Func,
    Info,
    Line,
    Module,
    Public,
    StackCfiDelta,
    StackCfiInit,
    StackWin,
}

impl fmt::Display for BreakpadRecordType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BreakpadRecordType::File => write!(f, "FILE"),
            BreakpadRecordType::Func => write!(f, "FUNC"),
            BreakpadRecordType::Info => write!(f, "INFO"),
            BreakpadRecordType::Line => write!(f, "LINE"),
            BreakpadRecordType::Module => write!(f, "MODULE"),
            BreakpadRecordType::Public => write!(f, "PUBLIC"),
            BreakpadRecordType::StackCfiDelta => write!(f, "STACK CFI"),
            BreakpadRecordType::StackCfiInit => write!(f, "STACK CFI INIT"),
            BreakpadRecordType::StackWin => write!(f, "STACK WIN"),
        }
    }
}

#[derive(Clone, Debug, Error)]
pub struct ParseBreakpadError2 {
    message: &'static str,
    input: String,
    position: usize,
}

impl fmt::Display for ParseBreakpadError2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;

        if f.alternate() {
            writeln!(f, "\n\n{}", self.input)?;
            write!(f, "{:width$}^", "", width = self.position)?;
        }

        Ok(())
    }
}

trait ParseRecord: Sized {
    const RECORD_TYPE: BreakpadRecordType;

    fn parse_str(input: &str) -> Result<Self, &'static str>;

    fn parse(data: &[u8]) -> Result<Self, BreakpadError> {
        let string = str::from_utf8(data)?;
        debug_assert!(!string.contains('\n'), "Illegal input: {}", string);

        Self::parse_str(string).map_err(|message| {
            let source = ParseBreakpadError2 {
                message,
                input: string.to_owned(),
                position: 0,
            };

            let kind = BreakpadErrorKind::Parse(Self::RECORD_TYPE);
            BreakpadError::new(kind, source)
        })
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub struct ParseBreakpadError {
    kind: ParseBreakpadErrorKind,
    input: String,
    context_input: Option<String>,
    context_record: Option<BreakpadRecordType>,
}

impl ParseBreakpadError {
    fn context_input(mut self, context_input: &str) -> Self {
        self.context_input = Some(context_input.to_string());
        self
    }

    fn context_record(mut self, context_record: BreakpadRecordType) -> Self {
        self.context_record = Some(context_record);
        self
    }
}

impl fmt::Display for ParseBreakpadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.kind {
            ParseBreakpadErrorKind::Arch => write!(f, "Invalid architecture: ")?,
            ParseBreakpadErrorKind::Id => write!(f, "Invalid id: ")?,
            ParseBreakpadErrorKind::Line => write!(f, "Invalid line number: ")?,
            ParseBreakpadErrorKind::MissingInput => write!(f, "Missing input for ")?,
            ParseBreakpadErrorKind::NumDec => write!(f, "Invalid decimal number: ")?,
            ParseBreakpadErrorKind::NumHex => write!(f, "Invalid hex number: ")?,
            ParseBreakpadErrorKind::Os => write!(f, "Invalid OS: ")?,
            ParseBreakpadErrorKind::StackWinRecordType => {
                write!(f, "Invalid stack win record type: ")?;
            }
        }

        writeln!(f, "{}", self.input)?;

        write!(f, "While trying to parse ")?;
        match self.context_record {
            Some(BreakpadRecordType::File) => write!(f, "a FILE")?,
            Some(BreakpadRecordType::Func) => write!(f, "a FUNC")?,
            Some(BreakpadRecordType::Info) => write!(f, "an INFO")?,
            Some(BreakpadRecordType::Line) => write!(f, "a LINE")?,
            Some(BreakpadRecordType::Module) => write!(f, "a MODULE")?,
            Some(BreakpadRecordType::Public) => write!(f, "a PUBLIC")?,
            Some(BreakpadRecordType::StackCfiDelta) => write!(f, "a STACK CFI")?,
            Some(BreakpadRecordType::StackCfiInit) => write!(f, "a STACK CFI INIT")?,
            Some(BreakpadRecordType::StackWin) => write!(f, "a STACK WIN")?,
            None => write!(f, "an unknown")?,
        }

        writeln!(f, " record:")?;
        if let Some(ref context_input) = self.context_input {
            writeln!(f, "{}", context_input)
        } else {
            Ok(())
        }
    }
}

/// The error type for [`BreakpadError`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BreakpadErrorKind {
    /// The symbol header (`MODULE` record) is missing.
    InvalidMagic,

    /// A part of the file is not encoded in valid UTF-8.
    BadEncoding,

    /// Parsing of a record failed.
    Parse(BreakpadRecordType),

    /// The debug id is not valid.
    InvalidDebugId,
}

impl fmt::Display for BreakpadErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMagic => write!(f, "missing breakpad symbol header"),
            Self::BadEncoding => write!(f, "bad utf-8 sequence"),
            Self::Parse(ty) => write!(f, "failed to parse {} record", ty),
            Self::InvalidDebugId => write!(f, "invalid debug id"),
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

impl From<ParseBreakpadError> for BreakpadError {
    fn from(e: ParseBreakpadError) -> Self {
        Self::new(BreakpadErrorKind::Parse, e)
    }
}

impl From<ParseDebugIdError> for BreakpadError {
    fn from(e: ParseDebugIdError) -> Self {
        Self::new(BreakpadErrorKind::InvalidDebugId, e)
    }
}

// TODO(ja): Test the parser

fn next(input: &str) -> Result<(&str, &str), &'static str> {
    if input.is_empty() {
        return Err("unexpected end of input");
    }

    Ok(match input.find(char::is_whitespace) {
        Some(split) => (&input[..split], input[split..].trim_start()),
        None => (&input, ""),
    })
}

fn expect<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
    let (a, rest) = next(input)?;
    (a == prefix).then(|| rest)
}

fn next_and<T, F>(input: &str, mut parser: F) -> Result<(T, &str), &'static str>
where
    F: FnMut(&str) -> Result<T, &'static str>,
{
    next(input).and_then(|(part, rest)| Ok((parser(part)?, rest)))
}

fn parse_hex64(input: &str) -> Result<u64, &'static str> {
    u64::from_str_radix(input, 16).or(Err("expected hex number"))
}

fn parse_u64(input: &str) -> Result<u64, &'static str> {
    input.parse().or(Err("expected number"))
}

fn done(input: &str) -> Result<(), &'static str> {
    match input {
        "" => Ok(()),
        _ => Err("unexpected trailing data"),
    }
}

fn num_hex_64(input: &str) -> ParseResult<u64> {
    u64::from_str_radix(input, 16).map_err(|_| ParseBreakpadError {
        input: input.to_string(),
        kind: ParseBreakpadErrorKind::NumHex,
        context_input: None,
        context_record: None,
    })
}

fn num_dec_64(input: &str) -> ParseResult<u64> {
    u64::from_str_radix(input, 10).map_err(|_| ParseBreakpadError {
        input: input.to_string(),
        kind: ParseBreakpadErrorKind::NumDec,
        context_input: None,
        context_record: None,
    })
}

fn num_hex_32(input: &str) -> ParseResult<u32> {
    u32::from_str_radix(input, 16).map_err(|_| ParseBreakpadError {
        input: input.to_string(),
        kind: ParseBreakpadErrorKind::NumHex,
        context_input: None,
        context_record: None,
    })
}

fn num_hex_16(input: &str) -> ParseResult<u16> {
    u16::from_str_radix(input, 16).map_err(|_| ParseBreakpadError {
        input: input.to_string(),
        kind: ParseBreakpadErrorKind::NumHex,
        context_input: None,
        context_record: None,
    })
}

fn line(input: &str) -> ParseResult<u64> {
    input
        .parse::<i64>()
        .map(|line| line.max(0) as u64)
        .map_err(|_| ParseBreakpadError {
            input: input.to_string(),
            kind: ParseBreakpadErrorKind::Line,
            context_input: None,
            context_record: None,
        })
}

fn hex_str(input: &str) -> ParseResult<&str> {
    if input.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(input)
    } else {
        Err(ParseBreakpadError {
            input: input.to_string(),
            kind: ParseBreakpadErrorKind::Id,
            context_input: None,
            context_record: None,
        })
    }
}

fn stack_win_record_type(input: &str) -> ParseResult<BreakpadStackWinRecordType> {
    match input {
        "0" => Ok(BreakpadStackWinRecordType::Fpo),
        "4" => Ok(BreakpadStackWinRecordType::FrameData),
        _ => Err(ParseBreakpadError {
            input: input.to_string(),
            kind: ParseBreakpadErrorKind::StackWinRecordType,
            context_input: None,
            context_record: None,
        }),
    }
}

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

// macro_rules! expect_input {
//     ($name:expr, $input:expr  $(,)?) => {{
//         $input.ok_or_else(|| ParseBreakpadError {
//             context_input: None,
//             context_record: None,
//             input: $name.to_string(),
//             kind: ParseBreakpadErrorKind::MissingInput,
//         })?
//     }};
// }

fn expect_input<'a>(name: &'static str, input: Option<&'a str>) -> ParseResult<&'a str> {
    input.ok_or_else(|| ParseBreakpadError {
        context_input: None,
        context_record: None,
        input: name.to_owned(),
        kind: ParseBreakpadErrorKind::MissingInput,
    })
}

impl<'d> BreakpadModuleRecord<'d> {
    /// Parses a module record from a single line.
    pub fn parse(data: &'d [u8]) -> Result<Self, BreakpadError> {
        let input = str::from_utf8(data)?;

        let go = |input: &'d str| {
            // Split off first line; the input might be an entire breakpad object file
            let input = expect_input("module record", input.lines().next())?;
            let current = expect_input("MODULE prefix", input.strip_prefix("MODULE"))?.trim_start();

            let mut parts = current.splitn(4, char::is_whitespace);

            let os = expect_input("os", parts.next())?;
            let arch = expect_input("arch", parts.next())?;
            let id = hex_str(expect_input("id", parts.next())?)?;
            let name = parts.next().unwrap_or(UNKNOWN_NAME);

            Ok(BreakpadModuleRecord { os, arch, id, name })
        };

        go(input).map_err(|e: ParseBreakpadError| {
            e.context_input(input)
                .context_record(BreakpadRecordType::Module)
                .into()
        })
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
        let input = str::from_utf8(data)?;
        debug_assert!(!input.contains('\n'), "Illegal input: {}", input);

        let go = |input: &'d str| {
            let mut current =
                expect_input("INFO prefix", input.strip_prefix("INFO "))?.trim_start();

            if let Some(rest) = current.strip_prefix("CODE_ID") {
                current = rest.trim_start();
                let mut parts = current.splitn(2, char::is_whitespace);
                let code_id = hex_str(expect_input("code_id", parts.next())?)?;

                let code_file = parts.next().unwrap_or("");
                Ok(BreakpadInfoRecord::CodeId { code_id, code_file })
            } else {
                let mut parts = current.splitn(2, char::is_whitespace);
                let scope = hex_str(expect_input("scope", parts.next())?)?;

                let info = expect_input("code_id", parts.next())?;
                Ok(BreakpadInfoRecord::Other { scope, info })
            }
        };

        go(input).map_err(|e: ParseBreakpadError| {
            e.context_input(input)
                .context_record(BreakpadRecordType::Info)
                .into()
        })
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
        let input = str::from_utf8(data)?;
        debug_assert!(!input.contains('\n'), "Illegal input: {}", input);

        let go = |input: &'d str| {
            let mut current = expect_input("FILE prefix", input.strip_prefix("FILE"))?.trim_start();
            let mut parts = current.splitn(2, char::is_whitespace);

            current = expect_input("id", parts.next())?;
            let id = num_dec_64(current)?;

            let name = parts.next().unwrap_or(UNKNOWN_NAME);

            Ok(BreakpadFileRecord { id, name })
        };

        go(input).map_err(|e: ParseBreakpadError| {
            e.context_input(input)
                .context_record(BreakpadRecordType::File)
                .into()
        })
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
        let input = str::from_utf8(data)?;
        debug_assert!(!input.contains('\n'), "Illegal input: {}", input);

        let go = |input: &'d str| {
            let mut current =
                expect_input("PUBLIC prefix", input.strip_prefix("PUBLIC"))?.trim_start();

            let multiple = if let Some(rest) = current.strip_prefix("m") {
                current = rest.trim_start();
                true
            } else {
                false
            };

            let mut parts = current.splitn(3, char::is_whitespace);

            current = expect_input("address", parts.next())?;
            let address = num_hex_64(current)?;

            current = expect_input("parameter_size", parts.next())?;
            let parameter_size = num_hex_64(current)?;

            let name = parts.next().unwrap_or(UNKNOWN_NAME);

            Ok(BreakpadPublicRecord {
                multiple,
                address,
                parameter_size,
                name,
            })
        };

        go(input).map_err(|e: ParseBreakpadError| {
            e.context_input(input)
                .context_record(BreakpadRecordType::Public)
                .into()
        })
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
    pub fn parse(data: &'d [u8]) -> Result<Self, BreakpadError> {
        let input = str::from_utf8(data)?;
        debug_assert!(!input.contains('\n'));

        let go = |input: &'d str| {
            let rest = expect(input, "FUNC").unwrap();

            let (multiple, rest) = match expect(rest, "m") {
                Some(new_rest) => (true, new_rest),
                None => (false, rest),
            };

            let (address, rest) = next_and(rest, parse_hex64).unwrap();
            let (size, rest) = next_and(rest, parse_hex64).unwrap();
            let (parameter_size, mut name) = next_and(rest, parse_hex64).unwrap();

            if name.is_empty() {
                name = UNKNOWN_NAME;
            }

            Ok(BreakpadFuncRecord {
                multiple,
                address,
                size,
                parameter_size,
                name,
                lines: Lines::default(),
            })
        };

        go(input).map_err(|e: ParseBreakpadError| {
            e.context_input(input)
                .context_record(BreakpadRecordType::Func)
                .into()
        })
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

        while let Some(line) = self.lines.next() {
            // Fast path: FUNC records are always before stack records. Once we encounter the
            // first stack record, we can therefore exit.
            if line.starts_with(b"STACK ") {
                break;
            }

            if !line.starts_with(b"FUNC ") {
                continue;
            }

            let mut record = BreakpadFuncRecord::parse(line);
            if let Ok(ref mut record) = record {
                record.lines = self.lines.clone();
            }
            return Some(record);
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
        ParseRecord::parse(data)
    }

    /// Resolves the filename for this record in the file map.
    pub fn filename<'d>(&self, file_map: &BreakpadFileMap<'d>) -> Option<&'d str> {
        file_map.get(&self.file_id).cloned()
    }
}

impl ParseRecord for BreakpadLineRecord {
    const RECORD_TYPE: BreakpadRecordType = BreakpadRecordType::Line;

    fn parse_str(input: &str) -> Result<Self, &'static str> {
        let (address, rest) = next_and(input, parse_hex64)?;
        let (size, rest) = next_and(rest, parse_hex64)?;
        let (line, rest) = next_and(rest, |input| {
            let line = input.parse::<i64>().or(Err("expected number"))?;
            Ok(line.max(0) as u64)
        })?;
        let (file_id, rest) = next_and(rest, parse_u64)?;
        done(rest)?;

        Ok(BreakpadLineRecord {
            address,
            size,
            line,
            file_id,
        })
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
        let input = str::from_utf8(data)?;
        debug_assert!(!input.contains('\n'), "Illegal input: {}", input);

        let go = |input: &'d str| {
            let mut current =
                expect_input("STACK CFI prefix", input.strip_prefix("STACK CFI"))?.trim_start();

            let mut parts = current.splitn(2, char::is_whitespace);

            current = expect_input("address", parts.next())?;
            let address = num_hex_64(current)?;

            let rules = expect_input("rules", parts.next())?;
            Ok(BreakpadStackCfiDeltaRecord { address, rules })
        };

        go(input).map_err(|e: ParseBreakpadError| {
            e.context_input(input)
                .context_record(BreakpadRecordType::StackCfiDelta)
                .into()
        })
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
        let input = str::from_utf8(data)?;
        debug_assert!(!input.contains('\n'), "Illegal input: {}", input);

        let go = |input: &'d str| {
            let mut current = expect_input(
                "STACK CFI INIT prefix",
                input.strip_prefix("STACK CFI INIT"),
            )?
            .trim_start();

            let mut parts = current.splitn(3, char::is_whitespace);

            current = expect_input("start", parts.next())?;
            let start = num_hex_64(current)?;

            current = expect_input("size", parts.next())?;
            let size = num_hex_64(current)?;

            let init_rules = expect_input("init_rules", parts.next())?;

            Ok(BreakpadStackCfiRecord {
                start,
                size,
                init_rules,
                deltas: Lines::default(),
            })
        };

        go(input).map_err(|e: ParseBreakpadError| {
            e.context_input(input)
                .context_record(BreakpadRecordType::StackCfiInit)
                .into()
        })
    }

    /// Returns an iterator over this record's delta records.
    pub fn deltas(&self) -> BreakpadStackCfiDeltaRecords<'d> {
        BreakpadStackCfiDeltaRecords {
            lines: self.deltas.clone(),
        }
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
/// [http://msdn.microsoft.com/en-us/library/bc5207xw%28VS.100%29.aspx]. Breakpad only deals with
/// types 0 (`FPO`) and 4 (`FrameData`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BreakpadStackWinRecordType {
    /// Frame pointer omitted; FPO info available.
    Fpo = 0,

    /// Frame pointer omitted; Frame data info available.
    FrameData = 4,
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
        let input = str::from_utf8(data)?;
        debug_assert!(!input.contains('\n'), "Illegal input: {}", input);

        let go = |input: &'d str| {
            let mut current =
                expect_input("STACK WIN prefix", input.strip_prefix("STACK WIN"))?.trim_start();

            let mut parts = current.splitn(11, char::is_whitespace);

            current = expect_input("ty", parts.next())?;
            let ty = stack_win_record_type(current)?;

            current = expect_input("code_start", parts.next())?;
            let code_start = num_hex_32(current)?;

            current = expect_input("code_size", parts.next())?;
            let code_size = num_hex_32(current)?;

            current = expect_input("prolog_size", parts.next())?;
            let prolog_size = num_hex_16(current)?;

            current = expect_input("epilog_size", parts.next())?;
            let epilog_size = num_hex_16(current)?;

            current = expect_input("params_size", parts.next())?;
            let params_size = num_hex_32(current)?;

            current = expect_input("saved_regs_size", parts.next())?;
            let saved_regs_size = num_hex_16(current)?;

            current = expect_input("locals_size", parts.next())?;
            let locals_size = num_hex_32(current)?;

            current = expect_input("max_stack_size", parts.next())?;
            let max_stack_size = num_hex_32(current)?;

            current = expect_input("has_program_string", parts.next())?;
            let has_program_string = current != "0";

            current = expect_input("program_string or uses_base_pointer", parts.next())?;

            let (uses_base_pointer, program_string) = if has_program_string {
                (false, Some(current))
            } else {
                (current != "0", None)
            };

            Ok(BreakpadStackWinRecord {
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
            })
        };

        go(input).map_err(|e: ParseBreakpadError| {
            e.context_input(input)
                .context_record(BreakpadRecordType::StackWin)
                .into()
        })
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
        BreakpadStackCfiRecord::parse(data)
            .map(Self::Cfi)
            .or_else(|_| BreakpadStackWinRecord::parse(data).map(Self::Win))
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

        let module = BreakpadModuleRecord::parse(header)?;

        Ok(BreakpadObject {
            id: module.id.parse()?,
            arch: module
                .arch
                .parse()
                // This unwrap cannot fail; the parser already only lets valid strings thrugh
                .unwrap(),
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
            if let Ok(BreakpadInfoRecord::CodeId { code_id, .. }) = result {
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
            func_records: self.func_records(),
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

    fn debug_session(&self) -> Result<Self::Session, BreakpadError> {
        self.debug_session()
    }

    fn has_unwind_info(&self) -> bool {
        self.has_unwind_info()
    }

    fn has_sources(&self) -> bool {
        self.has_sources()
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
pub struct BreakpadDebugSession<'data> {
    file_map: BreakpadFileMap<'data>,
    func_records: BreakpadFuncRecords<'data>,
}

impl<'data> BreakpadDebugSession<'data> {
    /// Returns an iterator over all functions in this debug file.
    pub fn functions(&self) -> BreakpadFunctionIterator<'_> {
        BreakpadFunctionIterator {
            file_map: &self.file_map,
            func_records: self.func_records.clone(),
        }
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

    fn source_by_path(&self, path: &str) -> Result<Option<Cow<'_, str>>, BreakpadError> {
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
    func_records: BreakpadFuncRecords<'s>,
}

impl<'s> BreakpadFunctionIterator<'s> {
    fn convert(&self, record: BreakpadFuncRecord<'s>) -> Result<Function<'s>, BreakpadError> {
        let mut lines = Vec::new();
        for line in record.lines() {
            let line = line?;
            let filename = line.filename(&self.file_map).unwrap_or_default();

            lines.push(LineInfo {
                address: line.address,
                size: Some(line.size),
                file: FileInfo::from_path(filename.as_bytes()),
                line: line.line,
            });
        }

        Ok(Function {
            address: record.address,
            size: record.size,
            name: Name::new(record.name, NameMangling::Unmangled, Language::Unknown),
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
    fn parse_module_record() {
        let string = b"MODULE Linux x86_64 492E2DD23CC306CA9C494EEF1533A3810 crash";
        let record = BreakpadModuleRecord::parse(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadModuleRecord {
       ⋮    os: "Linux",
       ⋮    arch: "x86_64",
       ⋮    id: "492E2DD23CC306CA9C494EEF1533A3810",
       ⋮    name: "crash",
       ⋮}
        "###);
    }

    #[test]
    fn parse_module_record_short_id() {
        // NB: This id is one character short, missing the age. DebugId can handle this, however.
        let string = b"MODULE Linux x86_64 6216C672A8D33EC9CF4A1BAB8B29D00E libdispatch.so";
        let record = BreakpadModuleRecord::parse(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadModuleRecord {
       ⋮    os: "Linux",
       ⋮    arch: "x86_64",
       ⋮    id: "6216C672A8D33EC9CF4A1BAB8B29D00E",
       ⋮    name: "libdispatch.so",
       ⋮}
        "###);
    }

    #[test]
    fn parse_file_record() {
        let string = b"FILE 37 /usr/include/libkern/i386/_OSByteOrder.h";
        let record = BreakpadFileRecord::parse(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadFileRecord {
       ⋮    id: 37,
       ⋮    name: "/usr/include/libkern/i386/_OSByteOrder.h",
       ⋮}
        "###);
    }

    #[test]
    fn parse_file_record_space() {
        let string = b"FILE 38 /usr/local/src/filename with spaces.c";
        let record = BreakpadFileRecord::parse(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadFileRecord {
       ⋮    id: 38,
       ⋮    name: "/usr/local/src/filename with spaces.c",
       ⋮}
        "###);
    }

    #[test]
    fn parse_func_record() {
        // Lines will be tested separately
        let string = b"FUNC 1730 1a 0 <name omitted>";
        let record = BreakpadFuncRecord::parse(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadFuncRecord {
       ⋮    multiple: false,
       ⋮    address: 5936,
       ⋮    size: 26,
       ⋮    parameter_size: 0,
       ⋮    name: "<name omitted>",
       ⋮}
        "###);
    }

    #[test]
    fn parse_func_record_multiple() {
        let string = b"FUNC m 1730 1a 0 <name omitted>";
        let record = BreakpadFuncRecord::parse(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadFuncRecord {
       ⋮    multiple: true,
       ⋮    address: 5936,
       ⋮    size: 26,
       ⋮    parameter_size: 0,
       ⋮    name: "<name omitted>",
       ⋮}
        "###);
    }

    #[test]
    fn parse_func_record_no_name() {
        let string = b"FUNC 0 f 0";
        let record = BreakpadFuncRecord::parse(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadFuncRecord {
       ⋮    multiple: false,
       ⋮    address: 0,
       ⋮    size: 15,
       ⋮    parameter_size: 0,
       ⋮    name: "<unknown>",
       ⋮}
        "###);
    }

    #[test]
    fn parse_line_record() {
        let string = b"1730 6 93 20";
        let record = BreakpadLineRecord::parse(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadLineRecord {
       ⋮    address: 5936,
       ⋮    size: 6,
       ⋮    line: 93,
       ⋮    file_id: 20,
       ⋮}
        "###);
    }

    #[test]
    fn parse_line_record_negative_line() {
        let string = b"e0fd10 5 -376 2225";
        let record = BreakpadLineRecord::parse(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
        BreakpadLineRecord {
            address: 14744848,
            size: 5,
            line: 0,
            file_id: 2225,
        }
        "###);
    }

    #[test]
    fn parse_public_record() {
        let string = b"PUBLIC 5180 0 __clang_call_terminate";
        let record = BreakpadPublicRecord::parse(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadPublicRecord {
       ⋮    multiple: false,
       ⋮    address: 20864,
       ⋮    parameter_size: 0,
       ⋮    name: "__clang_call_terminate",
       ⋮}
        "###);
    }

    #[test]
    fn parse_public_record_multiple() {
        let string = b"PUBLIC m 5180 0 __clang_call_terminate";
        let record = BreakpadPublicRecord::parse(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadPublicRecord {
       ⋮    multiple: true,
       ⋮    address: 20864,
       ⋮    parameter_size: 0,
       ⋮    name: "__clang_call_terminate",
       ⋮}
        "###);
    }

    #[test]
    fn parse_public_record_no_name() {
        let string = b"PUBLIC 5180 0";
        let record = BreakpadPublicRecord::parse(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadPublicRecord {
       ⋮    multiple: false,
       ⋮    address: 20864,
       ⋮    parameter_size: 0,
       ⋮    name: "<unknown>",
       ⋮}
        "###);
    }

    #[test]
    fn parse_stack_win_record() {
        let string =
            b"STACK WIN 4 371a c 0 0 0 0 0 0 1 $T0 .raSearch = $eip $T0 ^ = $esp $T0 4 + =";
        let record = BreakpadStackWinRecord::parse(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
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
        }
        "###);
    }

    #[test]
    fn parse_stack_cfi_init_record() {
        let string = b"STACK CFI INIT 1880 2d .cfa: $rsp 8 + .ra: .cfa -8 + ^";
        let record = BreakpadStackCfiRecord::parse(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
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
        }
        "###);
    }

    #[test]
    fn parse_stack_cfi_delta_record() {
        let string = b"STACK CFI 804c4b1 .cfa: $esp 8 + $ebp: .cfa 8 - ^";
        let record = BreakpadStackCfiDeltaRecord::parse(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
        BreakpadStackCfiDeltaRecord {
            address: 134530225,
            rules: ".cfa: $esp 8 + $ebp: .cfa 8 - ^",
        }
        "###);
    }
}
