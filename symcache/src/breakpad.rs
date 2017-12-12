use std::str::FromStr;

use uuid::Uuid;

use symbolic_common::{Arch, ErrorKind, Result};
use symbolic_debuginfo::Object;

#[derive(Debug)]
pub struct BreakpadModuleRecord<'input> {
    pub arch: Arch,
    pub uuid: Uuid,
    pub name: &'input [u8],
}

#[derive(Debug)]
pub struct BreakpadFileRecord<'input> {
    pub id: u16,
    pub name: &'input [u8],
}

#[derive(Debug)]
pub struct BreakpadLineRecord {
    pub address: u64,
    pub line: u16,
    pub file_id: u16,
}

#[derive(Debug)]
pub struct BreakpadFuncRecord<'input> {
    pub address: u64,
    pub size: u64,
    pub name: &'input [u8],
    pub lines: Vec<BreakpadLineRecord>,
}

#[derive(Debug)]
pub struct BreakpadPublicRecord<'input> {
    pub address: u64,
    pub size: u64,
    pub name: &'input [u8],
}

#[derive(Debug)]
pub struct BreakpadInfo<'input> {
    module: Option<BreakpadModuleRecord<'input>>,
    files: Vec<BreakpadFileRecord<'input>>,
    funcs: Vec<BreakpadFuncRecord<'input>>,
    syms: Vec<BreakpadPublicRecord<'input>>,
}

impl<'input> BreakpadInfo<'input> {
    pub fn from_object(object: &'input Object) -> Result<BreakpadInfo<'input>> {
        let mut info = BreakpadInfo {
            module: None,
            files: vec![],
            funcs: vec![],
            syms: vec![],
        };

        info.parse(object.as_bytes())?;
        Ok(info)
    }

    pub fn files(&self) -> &[BreakpadFileRecord] {
        self.files.as_slice()
    }

    pub fn functions(&self) -> &[BreakpadFuncRecord] {
        self.funcs.as_slice()
    }

    pub fn symbols(&self) -> &[BreakpadPublicRecord] {
        self.syms.as_slice()
    }

    fn parse(&mut self, bytes: &'input [u8]) -> Result<()> {
        // TODO(ja): Support windows line endings
        let lines = bytes.split(|b| *b == b'\n');
        let mut in_func = false;

        for line in lines {
            let mut words = line.splitn(1, |b| *b == b' ');
            let magic = words.next().unwrap_or(b"");
            let record = words.next().unwrap_or(b"");

            match magic {
                b"MODULE" => {
                    self.parse_module(record)?;
                    in_func = false;
                }
                b"FILE" => {
                    self.parse_file(record)?;
                    in_func = false;
                }
                b"FUNC" => {
                    self.parse_func(record)?;
                    in_func = true;
                }
                b"STACK" => {
                    self.parse_stack(record)?;
                    in_func = false;
                }
                b"PUBLIC" => {
                    self.parse_public(record)?;
                    in_func = false;
                }
                b"" => {
                    // Ignore empty lines
                }
                _ => {
                    if !in_func {
                        // No known magic and we don't expect a line record
                        return Err(ErrorKind::BadBreakpadSym("Unexpected line record").into());
                    }

                    // Pass the whole line down as there is no magic
                    self.parse_line(line)?;
                }
            }
        }

        Ok(())
    }

    /// Parses a breakpad MODULE record
    ///
    /// Syntax: "MODULE operatingsystem architecture id name"
    /// Example: "MODULE Linux x86 D3096ED481217FD4C16B29CD9BC208BA0 firefox-bin"
    /// see https://github.com/google/breakpad/blob/master/docs/symbol_files.md#module-records
    fn parse_module(&mut self, line: &'input [u8]) -> Result<()> {
        if self.module.is_some() {
            return Err(ErrorKind::BadBreakpadSym("Multiple MODULE records not supported").into());
        }

        let mut record = line.splitn(3, |b| *b == b' ');

        // Skip "os" field
        record.next();

        let arch = match record.next() {
            Some(word) => String::from_utf8_lossy(word),
            None => return Err(ErrorKind::BadBreakpadSym("Missing module arch").into()),
        };

        let uuid_hex = match record.next() {
            Some(word) => String::from_utf8_lossy(word),
            None => return Err(ErrorKind::BadBreakpadSym("Missing module uuid").into()),
        };

        let uuid = match Uuid::parse_str(&uuid_hex[0..31]) {
            Ok(uuid) => uuid,
            Err(_) => return Err(ErrorKind::Parse("Invalid breakpad uuid").into()),
        };

        let name = match record.next() {
            Some(word) => word,
            None => return Err(ErrorKind::BadBreakpadSym("Missing module name").into()),
        };

        self.module = Some(BreakpadModuleRecord {
            name: name,
            arch: Arch::from_breakpad(&arch)?,
            uuid: uuid,
        });

        Ok(())
    }

    /// Parses a breakpad FILE record
    ///
    /// Syntax: "FILE number name"
    /// Example: "FILE 2 /home/jimb/mc/in/browser/app/nsBrowserApp.cpp"
    /// see https://github.com/google/breakpad/blob/master/docs/symbol_files.md#file-records
    fn parse_file(&mut self, line: &'input [u8]) -> Result<()> {
        let mut record = line.splitn(1, |b| *b == b' ');

        let id = match record.next() {
            Some(text) => u16::from_str(&String::from_utf8_lossy(text))?,
            None => return Err(ErrorKind::Parse("Missing file ID").into()),
        };

        let name = match record.next() {
            Some(text) => text,
            None => return Err(ErrorKind::Parse("Missing file name").into()),
        };

        self.files.push(BreakpadFileRecord { id: id, name: name });
        Ok(())
    }

    /// Parses a breakpad FUNC record
    ///
    /// Syntax: "FUNC [m] address size parameter_size name"
    /// Example: "FUNC m c184 30 0 nsQueryInterfaceWithError::operator()(nsID const&, void**) const"
    /// see https://github.com/google/breakpad/blob/master/docs/symbol_files.md#func-records
    fn parse_func(&mut self, line: &'input [u8]) -> Result<()> {
        // Strip the optional "m" parameter; it has no meaning to us
        let line = if line.starts_with(b"m ") {
            &line[2..]
        } else {
            line
        };
        let mut record = line.splitn(3, |b| *b == b' ');

        let address = match record.next() {
            Some(text) => u64::from_str_radix(&String::from_utf8_lossy(text), 16)?,
            None => return Err(ErrorKind::Parse("Missing function address").into()),
        };

        let size = match record.next() {
            Some(text) => u64::from_str_radix(&String::from_utf8_lossy(text), 16)?,
            None => return Err(ErrorKind::Parse("Missing function size").into()),
        };

        // Skip the parameter_size field
        record.next();

        let name = match record.next() {
            Some(text) => text,
            None => return Err(ErrorKind::Parse("Missing function name").into()),
        };

        self.funcs.push(BreakpadFuncRecord {
            address: address,
            size: size,
            name: name,
            lines: vec![],
        });

        Ok(())
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
    fn parse_stack(&mut self, _line: &'input [u8]) -> Result<()> {
        // Ignored
        Ok(())
    }

    /// Parses a breakpad PUBLIC record
    ///
    /// Syntax: "PUBLIC [m] address parameter_size name"
    /// Example: "PUBLIC m 2160 0 Public2_1"
    /// see https://github.com/google/breakpad/blob/master/docs/symbol_files.md#public-records
    fn parse_public(&mut self, line: &'input [u8]) -> Result<()> {
        // Strip the optional "m" parameter; it has no meaning to us
        let line = if line.starts_with(b"m ") {
            &line[2..]
        } else {
            line
        };
        let mut record = line.splitn(3, |b| *b == b' ');

        let address = match record.next() {
            Some(text) => u64::from_str_radix(&String::from_utf8_lossy(text), 16)?,
            None => return Err(ErrorKind::Parse("Missing function address").into()),
        };

        // Skip the parameter_size field
        record.next();

        let name = match record.next() {
            Some(text) => text,
            None => return Err(ErrorKind::Parse("Missing function name").into()),
        };

        if let Some(last_rec) = self.syms.last_mut() {
            // The last PUBLIC record's size can now be computed
            last_rec.size = address.saturating_sub(last_rec.address);
        }

        self.syms.push(BreakpadPublicRecord {
            address: address,
            size: 0, // will be computed with the next PUBLIC record
            name: name,
        });

        Ok(())
    }

    /// Parses a breakpad line record (after funcs)
    ///
    /// Syntax: "address size line filenum"
    /// Example: "c184 7 59 4"
    /// see https://github.com/google/breakpad/blob/master/docs/symbol_files.md#line-records
    fn parse_line(&mut self, line: &'input [u8]) -> Result<()> {
        let func = match self.funcs.last_mut() {
            Some(func) => func,
            None => return Err(ErrorKind::BadBreakpadSym("Unexpected line record").into()),
        };

        let mut record = line.splitn(3, |b| *b == b' ');

        let address = match record.next() {
            Some(text) => u64::from_str_radix(&String::from_utf8_lossy(text), 16)?,
            None => return Err(ErrorKind::Parse("Missing line address").into()),
        };

        // Skip the size field
        record.next();

        let line_number = match record.next() {
            Some(text) => u16::from_str(&String::from_utf8_lossy(text))?,
            None => return Err(ErrorKind::Parse("Missing line number").into()),
        };

        let file_id = match record.next() {
            Some(text) => u16::from_str(&String::from_utf8_lossy(text))?,
            None => return Err(ErrorKind::Parse("Missing line file id").into()),
        };

        func.lines.push(BreakpadLineRecord {
            address: address,
            line: line_number,
            file_id: file_id,
        });

        Ok(())
    }
}
