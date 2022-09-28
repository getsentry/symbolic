use std::convert::Infallible;
use std::iter;

use symbolic_common::{Arch, CodeId, DebugId};
use symbolic_ppdb::{FormatError, PortablePdb};

use crate::{DebugSession, FileEntry, Function, ObjectKind, Parse, Symbol, SymbolMap};

pub type PortablePdbSymbolIterator = iter::Empty<Symbol<'static>>;
pub type PortablePdbFunctionIterator<'session> =
    iter::Empty<Result<Function<'session>, Infallible>>;
pub type PortablePdbFileIterator<'session> = iter::Empty<Result<FileEntry<'session>, Infallible>>;

#[derive(Debug)]
pub struct PortablePdbObject<'data> {
    data: &'data [u8],
    ppdb: PortablePdb<'data>,
}

impl<'data> PortablePdbObject<'data> {
    pub fn debug_id(&self) -> DebugId {
        // TODO: How to handle nonexistent pdb_ids?
        self.ppdb.pdb_id().unwrap()
    }

    pub fn code_id(&self) -> Option<CodeId> {
        None
    }

    pub fn arch(&self) -> Arch {
        Arch::Unknown
    }

    pub fn kind(&self) -> ObjectKind {
        ObjectKind::Debug
    }

    pub fn load_address(&self) -> u64 {
        0
    }

    pub fn has_symbols(&self) -> bool {
        false
    }

    pub fn symbols(&self) -> PortablePdbSymbolIterator {
        iter::empty()
    }

    pub fn symbol_map(&self) -> SymbolMap<'data> {
        SymbolMap::new()
    }

    pub fn has_debug_info(&self) -> bool {
        self.ppdb.has_debug_info()
    }

    pub fn debug_session(&self) -> Result<PortablePdbDebugSession, Infallible> {
        Ok(PortablePdbDebugSession)
    }

    pub fn has_unwind_info(&self) -> bool {
        false
    }

    pub fn has_sources(&self) -> bool {
        false
    }

    pub fn is_malformed(&self) -> bool {
        false
    }

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
