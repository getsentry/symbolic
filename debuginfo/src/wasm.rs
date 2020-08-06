//! Support for WASM Objects (WebAssembly).
use std::borrow::Cow;
use std::fmt;

use failure::Fail;
use walrus;

use symbolic_common::{Arch, AsSelf, CodeId, DebugId};

use crate::base::*;
use crate::dwarf::{Dwarf, DwarfDebugSession, DwarfError, DwarfSection, Endian};
use crate::private::Parse;

/// An error when dealing with [`WasmObject`](struct.WasmObject.html).
#[derive(Debug, Fail)]
pub enum WasmError {
    /// The module cannot be parsed.
    #[fail(display = "invalid wasm file")]
    InvalidObject,
}

/// Wasm object container (.wasm), used for executables and debug
/// companions on web and wasi.
///
/// This can only parse binary wasm file and not wast files.
pub struct WasmObject<'d> {
    wasm_module: walrus::Module,
    data: &'d [u8],
}

impl<'d> WasmObject<'d> {
    /// Tests whether the buffer could contain a WASM object.
    pub fn test(data: &[u8]) -> bool {
        match data.get(0..4) {
            Some(b"\x00asm") => true,
            _ => false,
        }
    }

    /// Tries to parse a WASM from the given slice.
    pub fn parse(data: &'d [u8]) -> Result<Self, WasmError> {
        let wasm_module = match walrus::Module::from_buffer(data) {
            Ok(module) => module,
            Err(_) => return Err(WasmError::InvalidObject),
        };
        Ok(WasmObject { wasm_module, data })
    }

    /// The container file format, which is always `FileFormat::Wasm`.
    pub fn file_format(&self) -> FileFormat {
        FileFormat::Wasm
    }

    /// The code identifier of this object.
    ///
    /// Wasm does not yet provide code IDs.
    pub fn code_id(&self) -> Option<CodeId> {
        // see `debug_id`
        for (_, section) in self.wasm_module.customs.iter() {
            if section.name() == "build_id" {
                return Some(CodeId::from_binary(&section.data(&Default::default())));
            }
        }
        None
    }

    /// The debug information identifier of a WASM file.
    ///
    /// Wasm does not yet provide debug IDs.
    pub fn debug_id(&self) -> DebugId {
        for (_, section) in self.wasm_module.customs.iter() {
            // this section is not defined yet
            // see https://github.com/WebAssembly/tool-conventions/issues/133
            if section.name() == "build_id" {
                return DebugId::from_guid_age(
                    section
                        .data(&Default::default())
                        .get(..16)
                        .unwrap_or(&[][..]),
                    0,
                )
                .ok()
                .unwrap_or_default();
            }
        }
        DebugId::nil()
    }

    /// The CPU architecture of this object.
    pub fn arch(&self) -> Arch {
        Arch::Wasm
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
    pub fn symbols<'o>(&'o self) -> WasmSymbolIterator<'d, 'o> {
        WasmSymbolIterator {
            funcs: (Box::new(self.wasm_module.funcs.iter()) as Box<dyn Iterator<Item = _>>)
                .peekable(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Returns an ordered map of symbols in the symbol table.
    pub fn symbol_map(&self) -> SymbolMap<'d> {
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
    pub fn debug_session(&self) -> Result<DwarfDebugSession<'d>, DwarfError> {
        let symbols = self.symbol_map();
        DwarfDebugSession::parse(self, symbols, self.load_address(), self.kind())
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

    /// Returns the raw data of the ELF file.
    pub fn data(&self) -> &'d [u8] {
        self.data
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

impl<'d> ObjectLike for WasmObject<'d> {
    type Error = DwarfError;
    type Session = DwarfDebugSession<'d>;

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
            if section.name().starts_with('.') && &section.name()[1..] == section_name {
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
pub struct WasmSymbolIterator<'d, 'o> {
    funcs: std::iter::Peekable<Box<dyn Iterator<Item = &'o walrus::Function> + 'o>>,
    _marker: std::marker::PhantomData<&'d [u8]>,
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

impl<'d, 'o> Iterator for WasmSymbolIterator<'d, 'o> {
    type Item = Symbol<'d>;

    fn next(&mut self) -> Option<Self::Item> {
        let func = self.funcs.next()?;
        let address = get_addr_of_function(func);
        let size = self
            .funcs
            .peek()
            .map_or(0, |func| match get_addr_of_function(func) {
                0 => 0,
                x => x - address,
            });
        Some(Symbol {
            name: func.name.as_ref().map(|x| Cow::Owned(x.clone())),
            address,
            size,
        })
    }
}
