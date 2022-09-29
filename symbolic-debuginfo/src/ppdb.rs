//! Support for Portable PDB Objects.
use std::convert::Infallible;
use std::iter;

use symbolic_common::{Arch, CodeId, DebugId};
use symbolic_ppdb::{FormatError, PortablePdb};

use crate::{DebugSession, FileEntry, Function, ObjectKind, Parse, Symbol, SymbolMap};

/// An iterator over symbols in a [`PortablePdbObject`].
pub type PortablePdbSymbolIterator = iter::Empty<Symbol<'static>>;
/// An iterator over functions in a [`PortablePdbObject`].
pub type PortablePdbFunctionIterator<'session> =
    iter::Empty<Result<Function<'session>, Infallible>>;
/// An iterator over files in a [`PortablePdbObject`].
pub type PortablePdbFileIterator<'session> = iter::Empty<Result<FileEntry<'session>, Infallible>>;

/// An object wrapping a Portable PDB file.
#[derive(Debug)]
pub struct PortablePdbObject<'data> {
    data: &'data [u8],
    ppdb: PortablePdb<'data>,
}

impl<'data> PortablePdbObject<'data> {
    /// Returns the Portable PDB contained in this object.
    pub fn portable_pdb(&self) -> &PortablePdb {
        &self.ppdb
    }

    /// The debug information identifier of a Portable PDB file.
    pub fn debug_id(&self) -> DebugId {
        self.ppdb.pdb_id().unwrap_or_default()
    }

    /// The code identifier of this object.
    ///
    /// Portable PDB does not provide code identifiers.
    pub fn code_id(&self) -> Option<CodeId> {
        None
    }

    /// The CPU architecture of this object.
    pub fn arch(&self) -> Arch {
        Arch::Unknown
    }

    /// The kind of this object.
    pub fn kind(&self) -> ObjectKind {
        ObjectKind::Debug
    }

    /// The address at which the image prefers to be loaded into memory.
    ///
    /// This is always 0 as this does not really apply to Portable PDB.
    pub fn load_address(&self) -> u64 {
        0
    }

    /// Returns true if this object exposes a public symbol table.
    pub fn has_symbols(&self) -> bool {
        false
    }

    /// Returns an iterator over symbols in the public symbol table.
    pub fn symbols(&self) -> PortablePdbSymbolIterator {
        iter::empty()
    }

    /// Returns an ordered map of symbols in the symbol table.
    pub fn symbol_map(&self) -> SymbolMap<'data> {
        SymbolMap::new()
    }

    /// Determines whether this object contains debug information.
    pub fn has_debug_info(&self) -> bool {
        self.ppdb.has_debug_info()
    }

    /// Constructs a debugging session.
    pub fn debug_session(&self) -> Result<PortablePdbDebugSession, Infallible> {
        Ok(PortablePdbDebugSession)
    }

    /// Determines whether this object contains stack unwinding information.
    pub fn has_unwind_info(&self) -> bool {
        false
    }

    /// Determines whether this object contains embedded source.
    pub fn has_sources(&self) -> bool {
        false
    }

    /// Determines whether this object is malformed and was only partially parsed.
    pub fn is_malformed(&self) -> bool {
        false
    }

    /// Returns the raw data of the Portable PDB file.
    pub fn data(&self) -> &'data [u8] {
        self.data
    }
}

impl<'data> Parse<'data> for PortablePdbObject<'data> {
    type Error = FormatError;

    fn test(data: &[u8]) -> bool {
        PortablePdb::peek(data)
    }

    fn parse(data: &'data [u8]) -> Result<Self, Self::Error> {
        let ppdb = PortablePdb::parse(data)?;
        Ok(Self { data, ppdb })
    }
}

/// A debug session for a Portable PDB object.
///
/// Currently this session is trivial and returns no files or functions.
pub struct PortablePdbDebugSession;

impl<'session> DebugSession<'session> for PortablePdbDebugSession {
    type Error = Infallible;

    type FunctionIterator = PortablePdbFunctionIterator<'session>;

    type FileIterator = PortablePdbFileIterator<'session>;

    fn functions(&'session self) -> Self::FunctionIterator {
        iter::empty()
    }

    fn files(&'session self) -> Self::FileIterator {
        iter::empty()
    }

    fn source_by_path(
        &self,
        _path: &str,
    ) -> Result<Option<std::borrow::Cow<'_, str>>, Self::Error> {
        Ok(None)
    }
}
