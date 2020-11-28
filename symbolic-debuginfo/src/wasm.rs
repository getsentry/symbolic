//! Support for WASM Objects (WebAssembly).
use std::borrow::Cow;
use std::fmt;

use thiserror::Error;

use symbolic_common::{Arch, AsSelf, CodeId, DebugId, Uuid};

use crate::base::*;
use crate::dwarf::{Dwarf, DwarfDebugSession, DwarfError, DwarfSection, Endian};
use crate::private::Parse;

/// An error when dealing with [`WasmObject`](struct.WasmObject.html).
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum WasmError {
    /// The module cannot be parsed.
    #[error("invalid WASM file")]
    BadObject,
}

/// Wasm object container (.wasm), used for executables and debug
/// companions on web and wasi.
///
/// This can only parse binary wasm file and not wast files.
pub struct WasmObject<'data> {
    wasm_module: walrus::Module,
    code_offset: u64,
    data: &'data [u8],
}

impl<'data> WasmObject<'data> {
    /// Tests whether the buffer could contain a WASM object.
    pub fn test(data: &[u8]) -> bool {
        data.starts_with(b"\x00asm")
    }

    /// Tries to parse a WASM from the given slice.
    pub fn parse(data: &'data [u8]) -> Result<Self, WasmError> {
        let wasm_module = match walrus::Module::from_buffer(data) {
            Ok(module) => module,
            Err(_) => return Err(WasmError::BadObject),
        };

        // we need to parse the file a second time to get the offset to the
        // code section as walrus does not expose that yet.
        let mut code_offset = 0;
        for payload in wasmparser::Parser::new(0).parse_all(data) {
            if let Ok(wasmparser::Payload::CodeSectionStart { range, .. }) = payload {
                code_offset = range.start as u64;
                break;
            }
        }

        Ok(WasmObject {
            wasm_module,
            data,
            code_offset,
        })
    }

    /// The container file format, which currently is always `FileFormat::Wasm`.
    pub fn file_format(&self) -> FileFormat {
        FileFormat::Wasm
    }

    fn get_raw_build_id(&self) -> Option<Cow<'_, [u8]>> {
        // this section is not defined yet
        // see https://github.com/WebAssembly/tool-conventions/issues/133
        for (_, section) in self.wasm_module.customs.iter() {
            if section.name() == "build_id" {
                return Some(section.data(&Default::default()));
            }
        }
        None
    }

    /// The code identifier of this object.
    ///
    /// Wasm does not yet provide code IDs.
    pub fn code_id(&self) -> Option<CodeId> {
        // see `debug_id`
        self.get_raw_build_id()
            .map(|data| CodeId::from_binary(&data))
    }

    /// The debug information identifier of a WASM file.
    ///
    /// Wasm does not yet provide debug IDs.
    pub fn debug_id(&self) -> DebugId {
        self.get_raw_build_id()
            .and_then(|data| {
                data.get(..16)
                    .and_then(|first_16| Uuid::from_slice(first_16).ok())
            })
            .map(DebugId::from_uuid)
            .unwrap_or_else(DebugId::nil)
    }

    /// The CPU architecture of this object.
    pub fn arch(&self) -> Arch {
        // TODO: we do not yet support wasm64 and thus always return wasm32 here.
        Arch::Wasm32
    }

    /// The kind of this object.
    pub fn kind(&self) -> ObjectKind {
        if self.wasm_module.funcs.iter().next().is_some() {
            ObjectKind::Library
        } else {
            ObjectKind::Debug
        }
    }

    /// The address at which the image prefers to be loaded into memory.
    ///
    /// This is always 0 as this does not really apply to WASM.
    pub fn load_address(&self) -> u64 {
        0
    }

    /// Determines whether this object exposes a public symbol table.
    pub fn has_symbols(&self) -> bool {
        true
    }

    /// Returns an iterator over symbols in the public symbol table.
    pub fn symbols(&self) -> WasmSymbolIterator<'data, '_> {
        let iterator = Box::new(self.wasm_module.funcs.iter()) as Box<dyn Iterator<Item = _>>;
        WasmSymbolIterator {
            funcs: iterator.peekable(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Returns an ordered map of symbols in the symbol table.
    pub fn symbol_map(&self) -> SymbolMap<'data> {
        self.symbols().collect()
    }

    /// Determines whether this object contains debug information.
    pub fn has_debug_info(&self) -> bool {
        for (_, section) in self.wasm_module.customs.iter() {
            if section.name() == ".debug_info" {
                return true;
            }
        }
        false
    }

    /// Constructs a debugging session.
    pub fn debug_session(&self) -> Result<DwarfDebugSession<'data>, DwarfError> {
        let symbols = self.symbol_map();
        // WASM is offset by the negative offset to the code section instead of the load address
        DwarfDebugSession::parse(self, symbols, -(self.code_offset() as i64), self.kind())
    }

    /// Determines whether this object contains stack unwinding information.
    pub fn has_unwind_info(&self) -> bool {
        for (_, section) in self.wasm_module.customs.iter() {
            if section.name() == ".debug_frame" {
                return true;
            }
        }
        false
    }

    /// Determines whether this object contains embedded source.
    pub fn has_sources(&self) -> bool {
        false
    }

    /// Returns the raw data of the WASM file.
    pub fn data(&self) -> &'data [u8] {
        self.data
    }

    /// Returns the offset of the code section.
    pub fn code_offset(&self) -> u64 {
        self.code_offset
    }
}

impl fmt::Debug for WasmObject<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WasmObject")
            .field("code_id", &self.code_id())
            .field("debug_id", &self.debug_id())
            .field("arch", &self.arch())
            .field("kind", &self.kind())
            .field("load_address", &format_args!("{:#x}", self.load_address()))
            .field("has_symbols", &self.has_symbols())
            .field("has_debug_info", &self.has_debug_info())
            .field("has_unwind_info", &self.has_unwind_info())
            .finish()
    }
}

impl<'slf, 'd: 'slf> AsSelf<'slf> for WasmObject<'d> {
    type Ref = WasmObject<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        self
    }
}

impl<'d> Parse<'d> for WasmObject<'d> {
    type Error = WasmError;

    fn test(data: &[u8]) -> bool {
        Self::test(data)
    }

    fn parse(data: &'d [u8]) -> Result<Self, WasmError> {
        Self::parse(data)
    }
}

impl<'data: 'object, 'object> ObjectLike<'data, 'object> for WasmObject<'data> {
    type Error = DwarfError;
    type Session = DwarfDebugSession<'data>;
    type SymbolIterator = WasmSymbolIterator<'data, 'object>;

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

    fn symbols(&'object self) -> Self::SymbolIterator {
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
}

impl<'d> Dwarf<'d> for WasmObject<'d> {
    fn endianity(&self) -> Endian {
        Endian::Little
    }

    fn raw_section(&self, section_name: &str) -> Option<DwarfSection<'d>> {
        for (_, section) in self.wasm_module.customs.iter() {
            if section.name().strip_prefix('.') == Some(section_name) {
                return Some(DwarfSection {
                    data: Cow::Owned(section.data(&Default::default()).into_owned()),
                    // XXX: what are these going to be?
                    address: 0,
                    offset: 0,
                    align: 4,
                });
            }
        }

        None
    }
}

/// An iterator over symbols in the WASM file.
///
/// Returned by [`WasmObject::symbols`](struct.WasmObject.html#method.symbols).
pub struct WasmSymbolIterator<'data, 'object> {
    funcs: std::iter::Peekable<Box<dyn Iterator<Item = &'object walrus::Function> + 'object>>,
    _marker: std::marker::PhantomData<&'data [u8]>,
}

fn get_addr_of_function(func: &walrus::Function) -> u64 {
    if let walrus::FunctionKind::Local(ref loc) = func.kind {
        let entry_block = loc.entry_block();
        let seq = loc.block(entry_block);
        seq.instrs.get(0).map_or(0, |x| x.1.data() as u64)
    } else {
        0
    }
}

impl<'data, 'object> Iterator for WasmSymbolIterator<'data, 'object> {
    type Item = Symbol<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let func = self.funcs.next()?;
            if let walrus::FunctionKind::Local(_) = func.kind {
                let address = get_addr_of_function(func);
                let size = self
                    .funcs
                    .peek()
                    .map_or(0, |func| match get_addr_of_function(func) {
                        0 => 0,
                        x => x - address,
                    });
                return Some(Symbol {
                    name: func.name.as_ref().map(|x| Cow::Owned(x.clone())),
                    address,
                    size,
                });
            }
        }
    }
}
