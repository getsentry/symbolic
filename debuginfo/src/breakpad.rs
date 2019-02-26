use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::str;

use failure::Fail;
use pest::Parser;
use pest_derive::Parser;

use symbolic_common::{derive_failure, Arch, AsSelf, DebugId, Name};

use crate::base::*;
use crate::private::{Lines, Parse};

/// Length at which the breakpad header will be capped.
///
/// This is a protection against reading an entire breakpad file at once if the first characters do
/// not contain a valid line break.
const BREAKPAD_HEADER_CAP: usize = 320;

#[derive(Debug, Fail)]
pub enum BreakpadErrorKind {
    #[fail(display = "missing breakpad symbol header")]
    InvalidMagic,
    #[fail(display = "bad file encoding")]
    BadEncoding(#[fail(cause)] str::Utf8Error),
    #[fail(display = "{}", _0)]
    BadSyntax(pest::error::Error<Rule>),
    #[fail(display = "{}", _0)]
    Parse(&'static str),
    #[fail(display = "processing of breakpad symbols failed")]
    ProcessingFailed,
}

derive_failure!(BreakpadError, BreakpadErrorKind);

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

#[derive(Debug, Parser)]
#[grammar = "breakpad.pest"]
struct BreakpadParser;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BreakpadModuleRecord<'d> {
    pub os: &'d str,
    pub arch: &'d str,
    pub id: &'d str,
    pub name: &'d str,
}

impl<'d> BreakpadModuleRecord<'d> {
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BreakpadFileRecord<'d> {
    pub id: u64,
    pub name: &'d str,
}

impl<'d> BreakpadFileRecord<'d> {
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

pub type BreakpadFileMap<'d> = BTreeMap<u64, &'d str>;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BreakpadPublicRecord<'d> {
    pub multiple: bool,
    pub address: u64,
    pub parameter_size: u64,
    pub name: &'d str,
}

impl<'d> BreakpadPublicRecord<'d> {
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

#[derive(Clone, Default)]
pub struct BreakpadFuncRecord<'d> {
    pub multiple: bool,
    pub address: u64,
    pub size: u64,
    pub parameter_size: u64,
    pub name: &'d str,
    lines: Lines<'d>,
}

impl<'d> BreakpadFuncRecord<'d> {
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BreakpadLineRecord {
    pub address: u64,
    pub size: u64,
    pub line: u64,
    pub file_id: u64,
}

impl BreakpadLineRecord {
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

    pub fn filename<'d>(&self, file_map: &BreakpadFileMap<'d>) -> Option<&'d str> {
        file_map.get(&self.file_id).cloned()
    }
}

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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BreakpadStackCfiRecord<'d> {
    pub text: &'d str,
}

impl<'d> BreakpadStackCfiRecord<'d> {
    pub fn parse(data: &'d [u8]) -> Result<Self, BreakpadError> {
        let string = str::from_utf8(data)?;
        let parsed = BreakpadParser::parse(Rule::stack_cfi, string)?
            .next()
            .unwrap();

        Self::from_pair(parsed)
    }

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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BreakpadStackWinRecord<'d> {
    pub text: &'d str,
}

impl<'d> BreakpadStackWinRecord<'d> {
    pub fn parse(data: &'d [u8]) -> Result<Self, BreakpadError> {
        let string = str::from_utf8(data)?;
        let parsed = BreakpadParser::parse(Rule::stack_win, string)?
            .next()
            .unwrap();

        Self::from_pair(parsed)
    }

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BreakpadStackRecord<'d> {
    Cfi(BreakpadStackCfiRecord<'d>),
    Win(BreakpadStackWinRecord<'d>),
}

impl<'d> BreakpadStackRecord<'d> {
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

pub struct BreakpadObject<'d> {
    id: DebugId,
    arch: Arch,
    module: BreakpadModuleRecord<'d>,
    data: &'d [u8],
}

impl<'d> BreakpadObject<'d> {
    pub fn test(data: &[u8]) -> bool {
        data.starts_with(b"MODULE ")
    }

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

    pub fn file_format(&self) -> FileFormat {
        FileFormat::Breakpad
    }

    pub fn id(&self) -> DebugId {
        self.id
    }

    pub fn arch(&self) -> Arch {
        self.arch
    }

    pub fn name(&self) -> &'d str {
        self.module.name
    }

    pub fn kind(&self) -> ObjectKind {
        ObjectKind::Debug
    }

    pub fn load_address(&self) -> u64 {
        0 // Breakpad rebases all addresses when dumping symbols
    }

    pub fn has_symbols(&self) -> bool {
        self.public_records().next().is_some()
    }

    pub fn symbols(&self) -> BreakpadSymbolIterator<'d> {
        BreakpadSymbolIterator {
            records: self.public_records(),
        }
    }

    pub fn symbol_map(&self) -> SymbolMap<'d> {
        self.symbols().collect()
    }

    pub fn has_debug_info(&self) -> bool {
        self.func_records().next().is_some()
    }

    pub fn debug_session(&self) -> Result<BreakpadDebugSession<'d>, BreakpadError> {
        Ok(BreakpadDebugSession {
            file_map: self.file_map(),
            func_records: self.func_records(),
        })
    }

    pub fn has_unwind_info(&self) -> bool {
        self.stack_records().next().is_some()
    }

    pub fn file_records(&self) -> BreakpadFileRecords<'d> {
        BreakpadFileRecords {
            lines: Lines::new(self.data),
            finished: false,
        }
    }

    pub fn file_map(&self) -> BreakpadFileMap<'d> {
        self.file_records()
            .filter_map(|result| result.ok())
            .map(|file| (file.id, file.name))
            .collect()
    }

    pub fn public_records(&self) -> BreakpadPublicRecords<'d> {
        BreakpadPublicRecords {
            lines: Lines::new(self.data),
            finished: false,
        }
    }

    pub fn func_records(&self) -> BreakpadFuncRecords<'d> {
        BreakpadFuncRecords {
            lines: Lines::new(self.data),
            finished: false,
        }
    }

    pub fn stack_records(&self) -> BreakpadStackRecords<'d> {
        BreakpadStackRecords {
            lines: Lines::new(self.data),
            finished: false,
        }
    }

    pub fn data(&self) -> &'d [u8] {
        self.data
    }
}

impl fmt::Debug for BreakpadObject<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("BreakpadObject")
            .field("id", &self.id())
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

    fn id(&self) -> DebugId {
        self.id()
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
}

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

pub struct BreakpadDebugSession<'d> {
    file_map: BreakpadFileMap<'d>,
    func_records: BreakpadFuncRecords<'d>,
}

impl<'d> DebugSession for BreakpadDebugSession<'d> {
    type Error = BreakpadError;

    fn functions(&mut self) -> Result<Vec<Function<'_>>, Self::Error> {
        let mut line_buf = Vec::new();
        let mut functions = Vec::new();

        for func in self.func_records.clone() {
            let func = func?;

            line_buf.clear();
            for line in func.lines() {
                let line = line?;
                let filename = line.filename(&self.file_map).unwrap_or_default();
                let (dir, name) = symbolic_common::split_path(filename);

                line_buf.push(LineInfo {
                    address: line.address,
                    file: FileInfo {
                        name: Cow::Borrowed(name),
                        dir: Cow::Borrowed(dir.unwrap_or_default()),
                    },
                    line: line.line,
                })
            }

            functions.push(Function {
                address: func.address,
                size: func.size,
                name: Name::from(func.name),
                compilation_dir: Cow::Borrowed(""),
                lines: line_buf.clone(),
                inlinees: Vec::new(),
                inline: false,
            });
        }

        Ok(functions)
    }
}

impl<'slf, 'd: 'slf> AsSelf<'slf> for BreakpadDebugSession<'d> {
    type Ref = BreakpadDebugSession<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        self
    }
}
