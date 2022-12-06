//! Support for Portable PDB Objects.
use std::borrow::Cow;
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
        Ok(PortablePdbDebugSession {
            ppdb: self.ppdb.clone(),
        })
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
pub struct PortablePdbDebugSession<'data> {
    ppdb: PortablePdb<'data>,
}

impl<'data> PortablePdbDebugSession<'data> {
    /// Returns an iterator over all functions in this debug file.
    pub fn functions(&self) -> PortablePdbFunctionIterator<'_> {
        iter::empty()
    }

    /// Returns an iterator over all source files in this debug file.
    pub fn files(&self) -> PortablePdbFileIterator<'_> {
        PortablePdbFileIterator::new(&self.ppdb)
    }

    /// Looks up a file's source contents by its full canonicalized path.
    ///
    /// The given path must be canonicalized.
    pub fn source_by_path(&self, _path: &str) -> Result<Option<Cow<'_, str>>, FormatError> {
        Ok(None)
    }
}

impl<'data, 'session> DebugSession<'session> for PortablePdbDebugSession<'data> {
    type Error = FormatError;
    type FunctionIterator = PortablePdbFunctionIterator<'session>;
    type FileIterator = PortablePdbFileIterator<'session>;

    fn functions(&'session self) -> Self::FunctionIterator {
        self.functions()
    }

    fn files(&'session self) -> Self::FileIterator {
        self.files()
    }

    fn source_by_path(&self, path: &str) -> Result<Option<Cow<'_, str>>, Self::Error> {
        self.source_by_path(path)
    }
}

/// An iterator over source files in a Portable PDB file.
pub struct PortablePdbFileIterator<'s> {
    ppdb: &'s PortablePdb<'s>,
    row: usize,
    size: usize,
}

impl<'s> PortablePdbFileIterator<'s> {
    fn new(ppdb: &'s PortablePdb<'s>) -> Self {
        PortablePdbFileIterator {
            ppdb,
            // ppdb.get_document(index) - index is 1-based
            row: 1,
            // Zero indicates the value is unknown and must be read during the first next() call.
            // We do it this way so that we can return a FormatError in case one occurs when determining the size.
            size: 0,
        }
    }
}

impl<'s> Iterator for PortablePdbFileIterator<'s> {
    type Item = Result<FileEntry<'s>, FormatError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.size == 0 {
            match self.ppdb.get_documents_count() {
                Ok(size) => {
                    debug_assert!(size != usize::MAX);
                    self.size = size;
                }
                Err(e) => {
                    return Some(Err(e));
                }
            }
        }

        if self.row > self.size {
            return None;
        }

        let index = self.row;
        self.row += 1;

        let document = match self.ppdb.get_document(index) {
            Ok(doc) => doc,
            Err(e) => {
                return Some(Err(e));
            }
        };
        Some(Ok(FileEntry::new(
            Cow::default(),
            FileInfo::from_path_owned(document.name.as_bytes()),
        )))
    }
}
