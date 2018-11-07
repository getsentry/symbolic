use std::fmt;
use std::str::FromStr;

use symbolic_common::types::{Arch, DebugId, ObjectKind};

use crate::object::{FatObject, Object};

/// An error returned when parsing breakpad files fails.
#[derive(Fail, Debug, Copy, Clone)]
#[fail(display = "invalid breakpad symbol: {}", _0)]
pub struct ParseBreakpadError(&'static str);

impl ParseBreakpadError {
    pub fn new(message: &'static str) -> ParseBreakpadError {
        ParseBreakpadError(message)
    }
}

/// A Breakpad symbol record.
#[derive(Debug, PartialEq)]
pub enum BreakpadRecord<'input> {
    /// Header record containing module information.
    Module(BreakpadModuleRecord<'input>),
    /// Source file declaration.
    File(BreakpadFileRecord<'input>),
    /// Source function declaration.
    Function(BreakpadFuncRecord<'input>),
    /// Source line mapping.
    Line(BreakpadLineRecord),
    /// Linker visible symbol.
    Public(BreakpadPublicRecord<'input>),
    /// Meta data record (e.g. Build ID)
    Info(&'input [u8]),
    /// Call Frame Information (CFI) record.
    Stack,
}

/// Breakpad module record containing general information on the file.
#[derive(PartialEq)]
pub struct BreakpadModuleRecord<'input> {
    pub arch: Arch,
    pub id: DebugId,
    pub name: &'input [u8],
}

impl<'input> fmt::Debug for BreakpadModuleRecord<'input> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("BreakpadModuleRecord")
            .field("arch", &self.arch)
            .field("id", &self.id)
            .field("name", &String::from_utf8_lossy(self.name))
            .finish()
    }
}

/// Breakpad file record declaring a source file.
#[derive(PartialEq)]
pub struct BreakpadFileRecord<'input> {
    pub id: u64,
    pub name: &'input [u8],
}

impl<'input> fmt::Debug for BreakpadFileRecord<'input> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("BreakpadFileRecord")
            .field("id", &self.id)
            .field("name", &String::from_utf8_lossy(self.name))
            .finish()
    }
}

/// Breakpad line record declaring the mapping of a memory address to file and
/// line number.
#[derive(Debug, PartialEq)]
pub struct BreakpadLineRecord {
    pub address: u64,
    pub line: u64,
    pub file_id: u64,
}

/// Breakpad function record declaring address and size of a source function.
#[derive(PartialEq)]
pub struct BreakpadFuncRecord<'input> {
    pub address: u64,
    pub size: u64,
    pub name: &'input [u8],
    pub lines: Vec<BreakpadLineRecord>,
}

impl<'input> fmt::Debug for BreakpadFuncRecord<'input> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("BreakpadFuncRecord")
            .field("address", &self.address)
            .field("size", &self.size)
            .field("name", &String::from_utf8_lossy(self.name))
            .field("lines", &self.lines)
            .finish()
    }
}

/// Breakpad public record declaring a linker-visible symbol.
#[derive(PartialEq)]
pub struct BreakpadPublicRecord<'input> {
    pub address: u64,
    pub size: u64,
    pub name: &'input [u8],
}

impl<'input> fmt::Debug for BreakpadPublicRecord<'input> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("BreakpadPublicRecord")
            .field("address", &self.address)
            .field("size", &self.size)
            .field("name", &String::from_utf8_lossy(self.name))
            .finish()
    }
}

/// Provides access to information in a breakpad file.
#[derive(Debug)]
pub(crate) struct BreakpadSym {
    id: DebugId,
    arch: Arch,
}

impl BreakpadSym {
    /// Parses a breakpad file header.
    ///
    /// Example:
    /// ```plain
    /// MODULE mac x86_64 13DA2547B1D53AF99F55ED66AF0C7AF70 Electron Framework
    /// ```
    pub fn parse(bytes: &[u8]) -> Result<BreakpadSym, ParseBreakpadError> {
        let mut words = bytes.splitn(5, |b| *b == b' ');

        match words.next() {
            Some(b"MODULE") => (),
            _ => return Err(ParseBreakpadError("bad file magic")),
        };

        // Operating system not needed
        words.next();

        let arch = words
            .next()
            .map(String::from_utf8_lossy)
            .ok_or_else(|| ParseBreakpadError("missing module arch"))?;

        let id = words
            .next()
            .map(String::from_utf8_lossy)
            .ok_or_else(|| ParseBreakpadError("missing module identifier"))?;

        Ok(BreakpadSym {
            arch: Arch::from_breakpad(&arch)
                .map_err(|_| ParseBreakpadError("unknown module architecture"))?,
            id: DebugId::from_breakpad(&id)
                .map_err(|_| ParseBreakpadError("invalid module identifier"))?,
        })
    }

    pub fn id(&self) -> DebugId {
        self.id
    }

    pub fn arch(&self) -> Arch {
        self.arch
    }
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
enum IterState {
    Started,
    Reading,
    Function,
}

/// An iterator over records in a Breakpad symbol file.
pub struct BreakpadRecords<'data> {
    lines: Box<Iterator<Item = &'data [u8]> + 'data>,
    state: IterState,
}

impl<'data> BreakpadRecords<'data> {
    fn from_bytes(bytes: &'data [u8]) -> BreakpadRecords<'data> {
        BreakpadRecords {
            lines: Box::new(bytes.split(|b| *b == b'\n')),
            state: IterState::Started,
        }
    }

    fn parse(&mut self, line: &'data [u8]) -> Result<BreakpadRecord<'data>, ParseBreakpadError> {
        let mut words = line.splitn(2, |b| *b == b' ');
        let magic = words.next().unwrap_or(b"");
        let record = words.next().unwrap_or(b"");

        match magic {
            b"MODULE" => {
                if self.state != IterState::Started {
                    return Err(ParseBreakpadError("unexpected module header"));
                }

                self.state = IterState::Reading;
                parse_module(record)
            }
            b"FILE" => {
                self.state = IterState::Reading;
                parse_file(record)
            }
            b"FUNC" => {
                self.state = IterState::Function;
                parse_func(record)
            }
            b"STACK" => {
                self.state = IterState::Reading;
                parse_stack(record)
            }
            b"PUBLIC" => {
                self.state = IterState::Reading;
                parse_public(record)
            }
            b"INFO" => {
                self.state = IterState::Reading;
                parse_info(record)
            }
            _ => {
                if self.state == IterState::Function {
                    // Pass the whole line down as there is no magic
                    parse_line(line)
                } else {
                    // No known magic and we don't expect a line record
                    Err(ParseBreakpadError("unexpected line record"))
                }
            }
        }
    }
}

impl<'data> Iterator for BreakpadRecords<'data> {
    type Item = Result<BreakpadRecord<'data>, ParseBreakpadError>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(next) = self.lines.next() {
            let mut len = next.len();
            while len > 0 && next[len - 1] == b'\r' {
                len -= 1;
            }

            if len > 0 {
                return Some(self.parse(&next[0..len]));
            }
        }

        None
    }
}

/// Gives access to Breakpad debugging information.
pub trait BreakpadData {
    /// Determines whether this `Object` contains Breakpad debugging information.
    fn has_breakpad_data(&self) -> bool;

    /// Returns an iterator over all records of the Breakpad symbol file.
    fn breakpad_records(&self) -> BreakpadRecords;
}

impl<'data> BreakpadData for Object<'data> {
    fn has_breakpad_data(&self) -> bool {
        self.kind() == ObjectKind::Breakpad
    }

    fn breakpad_records(&self) -> BreakpadRecords {
        BreakpadRecords::from_bytes(self.as_bytes())
    }
}

impl<'data> BreakpadData for FatObject<'data> {
    fn has_breakpad_data(&self) -> bool {
        self.kind() == ObjectKind::Breakpad
    }

    fn breakpad_records(&self) -> BreakpadRecords {
        BreakpadRecords::from_bytes(self.as_bytes())
    }
}

/// Parses a breakpad MODULE record.
///
/// Syntax: "MODULE operatingsystem architecture id name"
/// Example: "MODULE Linux x86 D3096ED481217FD4C16B29CD9BC208BA0 firefox-bin"
/// see <https://github.com/google/breakpad/blob/master/docs/symbol_files.md#module-records>
fn parse_module(line: &[u8]) -> Result<BreakpadRecord, ParseBreakpadError> {
    let mut record = line.splitn(4, |b| *b == b' ');

    // Skip "os" field
    record.next();

    let arch = record
        .next()
        .map(String::from_utf8_lossy)
        .ok_or_else(|| ParseBreakpadError("missing module arch"))?;

    let id = record
        .next()
        .map(String::from_utf8_lossy)
        .ok_or_else(|| ParseBreakpadError("missing module identifier"))?;

    let name = record
        .next()
        .ok_or_else(|| ParseBreakpadError("missing module name"))?;

    Ok(BreakpadRecord::Module(BreakpadModuleRecord {
        arch: Arch::from_breakpad(&arch)
            .map_err(|_| ParseBreakpadError("unknown module architecture"))?,
        id: DebugId::from_breakpad(&id)
            .map_err(|_| ParseBreakpadError("invalid module identifier"))?,
        name,
    }))
}

/// Parses a breakpad FILE record.
///
/// Syntax: "FILE number name"
/// Example: "FILE 2 /home/jimb/mc/in/browser/app/nsBrowserApp.cpp"
/// see <https://github.com/google/breakpad/blob/master/docs/symbol_files.md#file-records>
fn parse_file(line: &[u8]) -> Result<BreakpadRecord, ParseBreakpadError> {
    let mut record = line.splitn(2, |b| *b == b' ');

    let id = record
        .next()
        .map(String::from_utf8_lossy)
        .ok_or_else(|| ParseBreakpadError("missing file identifier"))?;

    let name = record
        .next()
        .ok_or_else(|| ParseBreakpadError("missing file name"))?;

    Ok(BreakpadRecord::File(BreakpadFileRecord {
        id: u64::from_str(&id).map_err(|_| ParseBreakpadError("invalid file identifier"))?,
        name,
    }))
}

/// Parses a breakpad FUNC record.
///
/// Syntax: "FUNC [m] address size parameter_size name"
/// Example: "FUNC m c184 30 0 nsQueryInterfaceWithError::operator()(nsID const&, void**) const"
/// see <https://github.com/google/breakpad/blob/master/docs/symbol_files.md#func-records>
fn parse_func(line: &[u8]) -> Result<BreakpadRecord, ParseBreakpadError> {
    // Strip the optional "m" parameter; it has no meaning to us
    let line = if line.starts_with(b"m ") {
        &line[2..]
    } else {
        line
    };
    let mut record = line.splitn(4, |b| *b == b' ');

    let address = record
        .next()
        .map(String::from_utf8_lossy)
        .ok_or_else(|| ParseBreakpadError("missing function address"))?;

    let size = record
        .next()
        .map(String::from_utf8_lossy)
        .ok_or_else(|| ParseBreakpadError("missing function size"))?;

    // Skip the parameter_size field
    record.next();

    let name = record
        .next()
        .ok_or_else(|| ParseBreakpadError("missing function name"))?;

    Ok(BreakpadRecord::Function(BreakpadFuncRecord {
        address: u64::from_str_radix(&address, 16)
            .map_err(|_| ParseBreakpadError("invalid function address"))?,
        size: u64::from_str_radix(&size, 16)
            .map_err(|_| ParseBreakpadError("invalid function size"))?,
        name,
        lines: vec![],
    }))
}

/// Parses a breakpad STACK record.
///
/// Can either be a STACK WIN record...
/// Syntax: "STACK WIN type rva code_size prologue_size epilogue_size parameter_size saved_register_
/// size local_size max_stack_size has_program_string program_string_OR_allocates_base_pointer"
/// Example: "STACK WIN 4 2170 14 1 0 0 0 0 0 1 $eip 4 + ^ = $esp $ebp 8 + = $ebp $ebp ^ ="
/// see <https://github.com/google/breakpad/blob/master/docs/symbol_files.md#stack-win-records>
///
/// ... or a STACK CFI record
/// Syntax: "STACK CFI INIT address size register1: expression1 register2: expression2 ..."
/// Example: "STACK CFI INIT 804c4b0 40 .cfa: $esp 4 + $eip: .cfa 4 - ^"
/// see <https://github.com/google/breakpad/blob/master/docs/symbol_files.md#stack-cfi-records>
fn parse_stack(_line: &[u8]) -> Result<BreakpadRecord, ParseBreakpadError> {
    // Ignored
    Ok(BreakpadRecord::Stack)
}

/// Parses a breakpad PUBLIC record.
///
/// Syntax: "PUBLIC [m] address parameter_size name"
/// Example: "PUBLIC m 2160 0 Public2_1"
/// see <https://github.com/google/breakpad/blob/master/docs/symbol_files.md#public-records>
fn parse_public(line: &[u8]) -> Result<BreakpadRecord, ParseBreakpadError> {
    // Strip the optional "m" parameter; it has no meaning to us
    let line = if line.starts_with(b"m ") {
        &line[2..]
    } else {
        line
    };
    let mut record = line.splitn(3, |b| *b == b' ');

    let address = record
        .next()
        .map(String::from_utf8_lossy)
        .ok_or_else(|| ParseBreakpadError("missing function address"))?;

    // Skip the parameter_size field
    record.next();

    let name = record
        .next()
        .ok_or_else(|| ParseBreakpadError("missing function name"))?;

    Ok(BreakpadRecord::Public(BreakpadPublicRecord {
        address: u64::from_str_radix(&address, 16)
            .map_err(|_| ParseBreakpadError("invalid function address"))?,
        size: 0, // will be computed with the next PUBLIC record
        name,
    }))
}

/// Parses a breakpad INFO record.
///
/// Syntax: "INFO text"
/// Example: "INFO CODE_ID C22813AC7D101E2FF2598697023E1F28"
/// no documentation available
fn parse_info(line: &[u8]) -> Result<BreakpadRecord, ParseBreakpadError> {
    Ok(BreakpadRecord::Info(line))
}

/// Parses a breakpad line record (after funcs).
///
/// Syntax: "address size line filenum"
/// Example: "c184 7 59 4"
/// see <https://github.com/google/breakpad/blob/master/docs/symbol_files.md#line-records>
fn parse_line(line: &[u8]) -> Result<BreakpadRecord, ParseBreakpadError> {
    let mut record = line.splitn(4, |b| *b == b' ');

    let address = record
        .next()
        .map(String::from_utf8_lossy)
        .ok_or_else(|| ParseBreakpadError("missing line address"))?;

    // Skip the size field
    record.next();

    let line = record
        .next()
        .map(String::from_utf8_lossy)
        .ok_or_else(|| ParseBreakpadError("missing line number"))?;

    let file_id = record
        .next()
        .map(String::from_utf8_lossy)
        .ok_or_else(|| ParseBreakpadError("missing line file id"))?;

    Ok(BreakpadRecord::Line(BreakpadLineRecord {
        address: u64::from_str_radix(&address, 16)
            .map_err(|_| ParseBreakpadError("invalid line address"))?,
        line: u64::from_str(&line).map_err(|_| ParseBreakpadError("invalid line number"))?,
        file_id: u64::from_str(&file_id).map_err(|_| ParseBreakpadError("invalid line file id"))?,
    }))
}

#[test]
fn test_parse_line() {
    let iter = BreakpadRecords::from_bytes(&b"\
        PUBLIC 2f30 0 google_breakpad::ExceptionHandler::DoDump(int, void const*, unsigned long)\n\
        FUNC 1000 114 0 google_breakpad::CrashGenerationClient::RequestDump(_EXCEPTION_POINTERS *,MDRawAssertionInfo *)\
    "[..]);
    let records: Vec<_> = iter.map(|x| x.unwrap()).collect();
    assert_eq!(records.len(), 2);
    assert_eq!(
        records[0],
        BreakpadRecord::Public(BreakpadPublicRecord {
            address: 12080,
            size: 0,
            name: &b"google_breakpad::ExceptionHandler::DoDump(int, void const*, unsigned long)"[..],
        })
    );
    assert_eq!(records[1], BreakpadRecord::Function(BreakpadFuncRecord {
        address: 4096,
        size: 276,
        name: &b"google_breakpad::CrashGenerationClient::RequestDump(_EXCEPTION_POINTERS *,MDRawAssertionInfo *)"[..],
        lines: vec![],
    }));
}
