use std::fmt;
use std::str::FromStr;

use symbolic_common::{Arch, Error, ErrorKind, ObjectKind, Result};

use object::{FatObject, Object};
use id::DebugId;

/// A Breakpad symbol record.
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
#[derive(Debug)]
pub struct BreakpadLineRecord {
    pub address: u64,
    pub line: u64,
    pub file_id: u64,
}

/// Breakpad function record declaring address and size of a source function.
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
    /// ```
    /// MODULE mac x86_64 13DA2547B1D53AF99F55ED66AF0C7AF70 Electron Framework
    /// ```
    pub fn parse(bytes: &[u8]) -> Result<BreakpadSym> {
        let mut words = bytes.splitn(5, |b| *b == b' ');

        match words.next() {
            Some(b"MODULE") => (),
            _ => return Err(ErrorKind::BadBreakpadSym("Invalid breakpad magic").into()),
        };

        // Operating system not needed
        words.next();

        let arch = match words.next() {
            Some(word) => String::from_utf8_lossy(word),
            None => return Err(ErrorKind::BadBreakpadSym("Missing breakpad arch").into()),
        };

        let id_hex = match words.next() {
            Some(word) => String::from_utf8_lossy(word),
            None => return Err(ErrorKind::BadBreakpadSym("Missing breakpad uuid").into()),
        };

        let id = match DebugId::from_breakpad(&id_hex) {
            Ok(id) => id,
            Err(_) => return Err(ErrorKind::Parse("Invalid breakpad uuid").into()),
        };

        Ok(BreakpadSym {
            id: id,
            arch: Arch::from_breakpad(arch.as_ref()),
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

    fn parse(&mut self, line: &'data [u8]) -> Result<BreakpadRecord<'data>> {
        let mut words = line.splitn(2, |b| *b == b' ');
        let magic = words.next().unwrap_or(b"");
        let record = words.next().unwrap_or(b"");

        match magic {
            b"MODULE" => {
                if self.state != IterState::Started {
                    return Err(ErrorKind::BadBreakpadSym("unexpected module header").into());
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
                    Err(ErrorKind::BadBreakpadSym("unexpected line record").into())
                }
            }
        }
    }
}

impl<'data> Iterator for BreakpadRecords<'data> {
    type Item = Result<BreakpadRecord<'data>>;

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
    fn breakpad_records<'input>(&'input self) -> BreakpadRecords<'input>;
}

impl<'data> BreakpadData for Object<'data> {
    fn has_breakpad_data(&self) -> bool {
        self.kind() == ObjectKind::Breakpad
    }

    fn breakpad_records<'input>(&'input self) -> BreakpadRecords<'input> {
        BreakpadRecords::from_bytes(self.as_bytes())
    }
}

impl<'data> BreakpadData for FatObject<'data> {
    fn has_breakpad_data(&self) -> bool {
        self.kind() == ObjectKind::Breakpad
    }

    fn breakpad_records<'input>(&'input self) -> BreakpadRecords<'input> {
        BreakpadRecords::from_bytes(self.as_bytes())
    }
}

/// Parses a breakpad MODULE record
///
/// Syntax: "MODULE operatingsystem architecture id name"
/// Example: "MODULE Linux x86 D3096ED481217FD4C16B29CD9BC208BA0 firefox-bin"
/// see https://github.com/google/breakpad/blob/master/docs/symbol_files.md#module-records
fn parse_module(line: &[u8]) -> Result<BreakpadRecord> {
    // if self.module.is_some() {
    //     return Err(ErrorKind::BadBreakpadSym("Multiple MODULE records not supported").into());
    // }

    let mut record = line.splitn(4, |b| *b == b' ');

    // Skip "os" field
    record.next();

    let arch = match record.next() {
        Some(word) => String::from_utf8_lossy(word),
        None => return Err(ErrorKind::BadBreakpadSym("missing module arch").into()),
    };

    let id_hex = match record.next() {
        Some(word) => String::from_utf8_lossy(word),
        None => return Err(ErrorKind::BadBreakpadSym("missing module id").into()),
    };

    let id = match DebugId::from_breakpad(&id_hex) {
        Ok(id) => id,
        Err(_) => return Err(ErrorKind::Parse("invalid breakpad id").into()),
    };

    let name = match record.next() {
        Some(word) => word,
        None => return Err(ErrorKind::BadBreakpadSym("missing module name").into()),
    };

    Ok(BreakpadRecord::Module(BreakpadModuleRecord {
        name: name,
        arch: Arch::from_breakpad(&arch),
        id: id,
    }))
}

/// Parses a breakpad FILE record
///
/// Syntax: "FILE number name"
/// Example: "FILE 2 /home/jimb/mc/in/browser/app/nsBrowserApp.cpp"
/// see https://github.com/google/breakpad/blob/master/docs/symbol_files.md#file-records
fn parse_file<'data>(line: &'data [u8]) -> Result<BreakpadRecord<'data>> {
    let mut record = line.splitn(2, |b| *b == b' ');

    let id = match record.next() {
        Some(text) => u64::from_str(&String::from_utf8_lossy(text))
            .map_err(|e| Error::with_chain(e, "invalid file id"))?,
        None => return Err(ErrorKind::Parse("missing file ID").into()),
    };

    let name = match record.next() {
        Some(text) => text,
        None => return Err(ErrorKind::Parse("missing file name").into()),
    };

    Ok(BreakpadRecord::File(BreakpadFileRecord {
        id: id,
        name: name,
    }))
}

/// Parses a breakpad FUNC record
///
/// Syntax: "FUNC [m] address size parameter_size name"
/// Example: "FUNC m c184 30 0 nsQueryInterfaceWithError::operator()(nsID const&, void**) const"
/// see https://github.com/google/breakpad/blob/master/docs/symbol_files.md#func-records
fn parse_func<'data>(line: &'data [u8]) -> Result<BreakpadRecord<'data>> {
    // Strip the optional "m" parameter; it has no meaning to us
    let line = if line.starts_with(b"m ") {
        &line[2..]
    } else {
        line
    };
    let mut record = line.splitn(4, |b| *b == b' ');

    let address = match record.next() {
        Some(text) => u64::from_str_radix(&String::from_utf8_lossy(text), 16)
            .map_err(|e| Error::with_chain(e, "invalid function address"))?,
        None => return Err(ErrorKind::Parse("missing function address").into()),
    };

    let size = match record.next() {
        Some(text) => u64::from_str_radix(&String::from_utf8_lossy(text), 16)
            .map_err(|e| Error::with_chain(e, "invalid function size"))?,
        None => return Err(ErrorKind::Parse("missing function size").into()),
    };

    // Skip the parameter_size field
    record.next();

    let name = match record.next() {
        Some(text) => text,
        None => return Err(ErrorKind::Parse("missing function name").into()),
    };

    Ok(BreakpadRecord::Function(BreakpadFuncRecord {
        address: address,
        size: size,
        name: name,
        lines: vec![],
    }))
}

/// Parses a breakpad STACK record
///
/// Can either be a STACK WIN record...
/// Syntax: "STACK WIN type rva code_size prologue_size epilogue_size parameter_size saved_register_size local_size max_stack_size has_program_string program_string_OR_allocates_base_pointer"
/// Example: "STACK WIN 4 2170 14 1 0 0 0 0 0 1 $eip 4 + ^ = $esp $ebp 8 + = $ebp $ebp ^ ="
/// see https://github.com/google/breakpad/blob/master/docs/symbol_files.md#stack-win-records
///
/// ... or a STACK CFI record
/// Syntax: "STACK CFI INIT address size register1: expression1 register2: expression2 ..."
/// Example: "STACK CFI INIT 804c4b0 40 .cfa: $esp 4 + $eip: .cfa 4 - ^"
/// see https://github.com/google/breakpad/blob/master/docs/symbol_files.md#stack-cfi-records
fn parse_stack<'data>(_line: &'data [u8]) -> Result<BreakpadRecord<'data>> {
    // Ignored
    Ok(BreakpadRecord::Stack)
}

/// Parses a breakpad PUBLIC record
///
/// Syntax: "PUBLIC [m] address parameter_size name"
/// Example: "PUBLIC m 2160 0 Public2_1"
/// see https://github.com/google/breakpad/blob/master/docs/symbol_files.md#public-records
fn parse_public<'data>(line: &'data [u8]) -> Result<BreakpadRecord<'data>> {
    // Strip the optional "m" parameter; it has no meaning to us
    let line = if line.starts_with(b"m ") {
        &line[2..]
    } else {
        line
    };
    let mut record = line.splitn(4, |b| *b == b' ');

    let address = match record.next() {
        Some(text) => u64::from_str_radix(&String::from_utf8_lossy(text), 16)
            .map_err(|e| Error::with_chain(e, "invalid symbol address"))?,
        None => return Err(ErrorKind::Parse("missing symbol address").into()),
    };

    // Skip the parameter_size field
    record.next();

    let name = match record.next() {
        Some(text) => text,
        None => return Err(ErrorKind::Parse("missing function name").into()),
    };

    Ok(BreakpadRecord::Public(BreakpadPublicRecord {
        address: address,
        size: 0, // will be computed with the next PUBLIC record
        name: name,
    }))
}

/// Parses a breakpad INFO record
///
/// Syntax: "INFO text"
/// Example: "INFO CODE_ID C22813AC7D101E2FF2598697023E1F28"
/// no documentation available
fn parse_info<'data>(line: &'data [u8]) -> Result<BreakpadRecord<'data>> {
    Ok(BreakpadRecord::Info(line))
}

/// Parses a breakpad line record (after funcs)
///
/// Syntax: "address size line filenum"
/// Example: "c184 7 59 4"
/// see https://github.com/google/breakpad/blob/master/docs/symbol_files.md#line-records
fn parse_line<'data>(line: &'data [u8]) -> Result<BreakpadRecord<'data>> {
    let mut record = line.splitn(4, |b| *b == b' ');

    let address = match record.next() {
        Some(text) => u64::from_str_radix(&String::from_utf8_lossy(text), 16)
            .map_err(|e| Error::with_chain(e, "invalid line address"))?,
        None => return Err(ErrorKind::Parse("Missing line address").into()),
    };

    // Skip the size field
    record.next();

    let line_number = match record.next() {
        Some(text) => u64::from_str(&String::from_utf8_lossy(text))
            .map_err(|e| Error::with_chain(e, "invalid line number"))?,
        None => return Err(ErrorKind::Parse("Missing line number").into()),
    };

    let file_id = match record.next() {
        Some(text) => u64::from_str(&String::from_utf8_lossy(text))
            .map_err(|e| Error::with_chain(e, "invalid line file id"))?,
        None => return Err(ErrorKind::Parse("missing line file id").into()),
    };

    Ok(BreakpadRecord::Line(BreakpadLineRecord {
        address: address,
        line: line_number,
        file_id: file_id,
    }))
}
