use std::borrow::Cow;
use std::io::Cursor;
use std::sync::Arc;

use failure::Fail;
use goblin::{error::Error as GoblinError, pe};

use symbolic_common::{Arch, DebugId, Uuid};

use crate::base::*;
use crate::private::Parse;

#[derive(Debug, Fail)]
pub enum PeError {
    #[fail(display = "invalid PE file")]
    Goblin(#[fail(cause)] GoblinError),
}

#[derive(Clone, Debug)]
pub struct PeObject<'d> {
    pe: Arc<pe::PE<'d>>,
    data: &'d [u8],
}

impl<'d> PeObject<'d> {
    pub fn test(data: &[u8]) -> bool {
        match goblin::peek(&mut Cursor::new(data)) {
            Ok(goblin::Hint::PE) => true,
            _ => false,
        }
    }

    pub fn parse(data: &'d [u8]) -> Result<Self, PeError> {
        pe::PE::parse(data)
            .map(|pe| PeObject {
                pe: Arc::new(pe),
                data,
            })
            .map_err(PeError::Goblin)
    }

    pub fn file_format(&self) -> FileFormat {
        FileFormat::Pe
    }

    pub fn id(&self) -> DebugId {
        self.pe
            .debug_data
            .as_ref()
            .and_then(|debug_data| debug_data.codeview_pdb70_debug_info.as_ref())
            .and_then(|debug_info| {
                // PE always stores the signature with little endian UUID fields.
                // Convert to network byte order (big endian) to match the
                // Breakpad processor's expectations.
                let mut data = debug_info.signature;
                data[0..4].reverse(); // uuid field 1
                data[4..6].reverse(); // uuid field 2
                data[6..8].reverse(); // uuid field 3

                let uuid = Uuid::from_slice(&data).ok()?;
                Some(DebugId::from_parts(uuid, debug_info.age))
            })
            .unwrap_or_default()
    }

    pub fn arch(&self) -> Arch {
        let machine = self.pe.header.coff_header.machine;
        crate::pdb::arch_from_machine(machine.into())
    }

    pub fn kind(&self) -> ObjectKind {
        if self.pe.is_lib {
            ObjectKind::Library
        } else {
            ObjectKind::Executable
        }
    }

    pub fn load_address(&self) -> u64 {
        self.pe.image_base as u64
    }

    pub fn has_symbols(&self) -> bool {
        !self.pe.exports.is_empty()
    }

    pub fn symbols(&self) -> PeSymbolIterator<'d, '_> {
        PeSymbolIterator {
            exports: self.pe.exports.iter(),
        }
    }

    pub fn symbol_map(&self) -> SymbolMap<'d> {
        self.symbols().collect()
    }

    pub fn data(&self) -> &'d [u8] {
        self.data
    }
}

impl<'d> Parse<'d> for PeObject<'d> {
    type Error = PeError;

    fn test(data: &[u8]) -> bool {
        Self::test(data)
    }

    fn parse(data: &'d [u8]) -> Result<Self, PeError> {
        Self::parse(data)
    }
}

impl ObjectLike for PeObject<'_> {
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
}

impl Debugging for PeObject<'_> {
    type Error = NeverError;
    type Session = NoDebugSession;

    fn has_debug_info(&self) -> bool {
        false
    }

    fn debug_session(&self) -> Result<Self::Session, Self::Error> {
        Ok(NoDebugSession)
    }
}

pub struct PeSymbolIterator<'d, 'o> {
    exports: std::slice::Iter<'o, pe::export::Export<'d>>,
}

impl<'d, 'o> Iterator for PeSymbolIterator<'d, 'o> {
    type Item = Symbol<'d>;

    fn next(&mut self) -> Option<Self::Item> {
        self.exports.next().map(|export| Symbol {
            name: export.name.map(Cow::Borrowed),
            address: export.rva as u64,
            size: export.size as u64,
        })
    }
}
