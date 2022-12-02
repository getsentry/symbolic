//! Support for Portable PDB Objects.
use std::fmt;
use std::iter;

use symbolic_common::{Arch, CodeId, DebugId};
use symbolic_ppdb::{FormatError, PortablePdb};

use crate::base::*;

/// An iterator over symbols in a [`PortablePdbObject`].
pub type PortablePdbSymbolIterator<'data> = iter::Empty<Symbol<'data>>;
/// An iterator over functions in a [`PortablePdbObject`].
pub type PortablePdbFunctionIterator<'session> =
    iter::Empty<Result<Function<'session>, FormatError>>;

/// An object wrapping a Portable PDB file.
pub struct PortablePdbObject<'data> {
    data: &'data [u8],
    ppdb: PortablePdb<'data>,
}

impl<'data> PortablePdbObject<'data> {
    /// Returns the Portable PDB contained in this object.
    pub fn portable_pdb(&self) -> &PortablePdb {
        &self.ppdb
    }

    /// Returns the raw data of the Portable PDB file.
    pub fn data(&self) -> &'data [u8] {
        self.data
    }
}

impl<'data: 'object, 'object> ObjectLike<'data, 'object> for PortablePdbObject<'data> {
    type Error = FormatError;
    type Session = PortablePdbDebugSession<'data>;
    type SymbolIterator = PortablePdbSymbolIterator<'data>;

    /// The debug information identifier of a Portable PDB file.
    fn debug_id(&self) -> DebugId {
        self.ppdb.pdb_id().unwrap_or_default()
    }

    /// The code identifier of this object.
    ///
    /// Portable PDB does not provide code identifiers.
    fn code_id(&self) -> Option<CodeId> {
        None
    }

    /// The CPU architecture of this object.
    fn arch(&self) -> Arch {
        Arch::Unknown
    }

    /// The kind of this object.
    fn kind(&self) -> ObjectKind {
        ObjectKind::Debug
    }

    /// The address at which the image prefers to be loaded into memory.
    ///
    /// This is always 0 as this does not really apply to Portable PDB.
    fn load_address(&self) -> u64 {
        0
    }

    /// Returns true if this object exposes a public symbol table.
    fn has_symbols(&self) -> bool {
        false
    }

    /// Returns an iterator over symbols in the public symbol table.
    fn symbols(&self) -> PortablePdbSymbolIterator<'data> {
        iter::empty()
    }

    /// Returns an ordered map of symbols in the symbol table.
    fn symbol_map(&self) -> SymbolMap<'data> {
        SymbolMap::new()
    }

    /// Determines whether this object contains debug information.
    fn has_debug_info(&self) -> bool {
        self.ppdb.has_debug_info()
    }

    /// Constructs a debugging session.
    fn debug_session(&self) -> Result<PortablePdbDebugSession<'data>, FormatError> {
        Ok(PortablePdbDebugSession { ppdb: &self.ppdb })
    }

    /// Determines whether this object contains stack unwinding information.
    fn has_unwind_info(&self) -> bool {
        false
    }

    /// Determines whether this object contains embedded source.
    fn has_sources(&self) -> bool {
        false
    }

    /// Determines whether this object is malformed and was only partially parsed.
    fn is_malformed(&self) -> bool {
        false
    }

    /// The container file format, which currently is always `FileFormat::PortablePdb`.
    fn file_format(&self) -> FileFormat {
        FileFormat::PortablePdb
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

impl fmt::Debug for PortablePdbObject<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PortablePdbObject")
            .field("portable_pdb", &self.portable_pdb())
            .finish()
    }
}

/// A debug session for a Portable PDB object.
///
/// Currently this session is trivial and returns no files or functions.
pub struct PortablePdbDebugSession<'data> {
    ppdb: &'data PortablePdb<'data>,
}

impl<'session> DebugSession<'session> for PortablePdbDebugSession<'session> {
    type Error = FormatError;

    type FunctionIterator = PortablePdbFunctionIterator<'session>;

    type FileIterator = PortablePdbFileIterator<'session>;

    fn functions(&'session self) -> Self::FunctionIterator {
        iter::empty()
    }

    fn files(&'session self) -> Self::FileIterator {
        PortablePdbFileIterator {
            ppdb: self.ppdb,
            row: 0,
            size: 1,
        }
    }

    fn source_by_path(
        &self,
        _path: &str,
    ) -> Result<Option<std::borrow::Cow<'_, str>>, Self::Error> {
        Ok(None)
    }
}

/// An iterator over source files in a DWARF file.
pub struct PortablePdbFileIterator<'s> {
    ppdb: &'s PortablePdb<'s>,
    row: usize,
    size: usize,
}

impl<'s> Iterator for PortablePdbFileIterator<'s> {
    type Item = Result<FileEntry<'s>, FormatError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.row >= self.size {
            return None;
        }

        self.row += 1;
        let document = self.ppdb.get_document(self.row - 1).ok()?;
        Some(Ok(FileEntry {
            compilation_dir: &[],
            info: FileInfo::from_path(document.name.as_bytes()),
        }))
    }
}
