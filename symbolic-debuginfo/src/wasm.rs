//! Support for WASM Objects (WebAssembly).
use std::borrow::Cow;
use std::fmt;

use thiserror::Error;

use symbolic_common::{Arch, AsSelf, CodeId, DebugId, Uuid};

use crate::base::*;
use crate::dwarf::{Dwarf, DwarfDebugSession, DwarfError, DwarfSection, Endian};

mod parser;

/// An error when dealing with [`WasmObject`](struct.WasmObject.html).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WasmError {
    /// Failed to read data from a WASM binary
    #[error("invalid wasm file")]
    Read(#[from] wasmparser::BinaryReaderError),
    /// A function in the WASM binary referenced an unknown type
    #[error("function references unknown type")]
    UnknownFunctionType,
}

/// Wasm object container (.wasm), used for executables and debug
/// companions on web and wasi.
///
/// This can only parse binary wasm file and not wast files.
pub struct WasmObject<'data> {
    dwarf_sections: Vec<(&'data str, &'data [u8])>,
    funcs: Vec<Symbol<'data>>,
    build_id: Option<&'data [u8]>,
    data: &'data [u8],
    code_offset: u64,
    kind: ObjectKind,
}

impl<'data> WasmObject<'data> {
    /// Tests whether the buffer could contain a WASM object.
    pub fn test(data: &[u8]) -> bool {
        data.starts_with(b"\x00asm")
    }

    /// The container file format, which currently is always `FileFormat::Wasm`.
    pub fn file_format(&self) -> FileFormat {
        FileFormat::Wasm
    }

    /// The code identifier of this object.
    ///
    /// Wasm does not yet provide code IDs.
    #[inline]
    pub fn code_id(&self) -> Option<CodeId> {
        self.build_id.map(CodeId::from_binary)
    }

    /// The debug information identifier of a WASM file.
    ///
    /// Wasm does not yet provide debug IDs.
    #[inline]
    pub fn debug_id(&self) -> DebugId {
        self.build_id
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
    #[inline]
    pub fn kind(&self) -> ObjectKind {
        self.kind
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
        WasmSymbolIterator {
            funcs: self.funcs.clone().into_iter(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Returns an ordered map of symbols in the symbol table.
    pub fn symbol_map(&self) -> SymbolMap<'data> {
        self.symbols().collect()
    }

    /// Determines whether this object contains debug information.
    #[inline]
    pub fn has_debug_info(&self) -> bool {
        self.dwarf_sections
            .iter()
            .any(|(name, _)| *name == ".debug_info")
    }

    /// Constructs a debugging session.
    pub fn debug_session(&self) -> Result<DwarfDebugSession<'data>, DwarfError> {
        let symbols = self.symbol_map();
        // WASM is offset by the negative offset to the code section instead of the load address
        DwarfDebugSession::parse(self, symbols, -(self.code_offset() as i64), self.kind())
    }

    /// Determines whether this object contains stack unwinding information.
    #[inline]
    pub fn has_unwind_info(&self) -> bool {
        self.dwarf_sections
            .iter()
            .any(|(name, _)| *name == ".debug_frame")
    }

    /// Determines whether this object contains embedded source.
    pub fn has_sources(&self) -> bool {
        false
    }

    /// Determines whether this object is malformed and was only partially parsed
    pub fn is_malformed(&self) -> bool {
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
            .field("is_malformed", &self.is_malformed())
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

    fn is_malformed(&self) -> bool {
        self.is_malformed()
    }
}

impl<'data> Dwarf<'data> for WasmObject<'data> {
    fn endianity(&self) -> Endian {
        Endian::Little
    }

    fn raw_section(&self, section_name: &str) -> Option<DwarfSection<'data>> {
        self.dwarf_sections.iter().find_map(|(name, data)| {
            if name.strip_prefix('.') == Some(section_name) {
                Some(DwarfSection {
                    data: Cow::Borrowed(data),
                    // XXX: what are these going to be?
                    address: 0,
                    offset: 0,
                    align: 4,
                })
            } else {
                None
            }
        })
    }
}

/// An iterator over symbols in the WASM file.
///
/// Returned by [`WasmObject::symbols`](struct.WasmObject.html#method.symbols).
pub struct WasmSymbolIterator<'data, 'object> {
    funcs: std::vec::IntoIter<Symbol<'data>>,
    _marker: std::marker::PhantomData<&'object u8>,
}

impl<'data, 'object> Iterator for WasmSymbolIterator<'data, 'object> {
    type Item = Symbol<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        self.funcs.next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_header() {
        let data = b"\x00asm    ";

        assert!(WasmObject::parse(data).is_err());
    }
}
