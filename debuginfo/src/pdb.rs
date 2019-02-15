use std::io::Cursor;
use std::marker::PhantomData;
use std::sync::Arc;

use failure::Fail;
use fallible_iterator::FallibleIterator;
use pdb::MachineType;

use symbolic_common::{Arch, DebugId, Uuid};

use crate::base::*;
use crate::private::Parse;

type Pdb<'d> = pdb::PDB<'d, Cursor<&'d [u8]>>;

const MAGIC_BIG: &[u8] = b"Microsoft C/C++ MSF 7.00\r\n\x1a\x44\x53\x00\x00\x00";

#[derive(Debug, Fail)]
pub enum PdbError {
    #[fail(display = "invalid PDB file")]
    Pdb(#[fail(cause)] pdb::Error),
}

#[derive(Clone, Debug)]
pub struct PdbObject<'d> {
    pdb: Arc<Pdb<'d>>,
    debug_info: Arc<pdb::DebugInformation<'d>>,
    pdb_info: Arc<pdb::PDBInformation<'d>>,
    public_syms: Arc<pdb::SymbolTable<'d>>,
    data: &'d [u8],
}

impl<'d> PdbObject<'d> {
    pub fn test(data: &[u8]) -> bool {
        // NOTE: "Microsoft C/C++ program database 2.00" is not supported.
        data.starts_with(MAGIC_BIG)
    }

    pub fn parse(data: &'d [u8]) -> Result<Self, PdbError> {
        let mut pdb = Pdb::open(Cursor::new(data)).map_err(PdbError::Pdb)?;
        let dbi = pdb.debug_information().map_err(PdbError::Pdb)?;
        let pdbi = pdb.pdb_information().map_err(PdbError::Pdb)?;
        let pubi = pdb.global_symbols().map_err(PdbError::Pdb)?;

        Ok(PdbObject {
            pdb: Arc::new(pdb),
            debug_info: Arc::new(dbi),
            pdb_info: Arc::new(pdbi),
            public_syms: Arc::new(pubi),
            data,
        })
    }

    pub fn file_format(&self) -> FileFormat {
        FileFormat::Pdb
    }

    pub fn id(&self) -> DebugId {
        match Uuid::from_slice(&self.pdb_info.guid.as_bytes()[..]) {
            Ok(uuid) => DebugId::from_parts(uuid, self.pdb_info.age),
            Err(_) => DebugId::default(),
        }
    }

    pub fn arch(&self) -> Arch {
        self.debug_info
            .machine_type()
            .ok()
            .map(arch_from_machine)
            .unwrap_or_default()
    }

    pub fn kind(&self) -> ObjectKind {
        ObjectKind::Debug
    }

    pub fn load_address(&self) -> u64 {
        unimplemented!()
    }

    pub fn has_symbols(&self) -> bool {
        unimplemented!()
    }

    pub fn symbols(&self) -> PdbSymbolIterator<'d, '_> {
        PdbSymbolIterator {
            symbols: self.public_syms.iter(),
            _ph: PhantomData,
        }
    }

    pub fn symbol_map(&self) -> SymbolMap<'d> {
        self.symbols().collect()
    }
}

impl<'d> Parse<'d> for PdbObject<'d> {
    type Error = PdbError;

    fn test(data: &[u8]) -> bool {
        Self::test(data)
    }

    fn parse(data: &'d [u8]) -> Result<Self, PdbError> {
        Self::parse(data)
    }
}

impl ObjectLike for PdbObject<'_> {
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

impl Debugging for PdbObject<'_> {
    type Error = NeverError;
    type Session = NoDebugSession;

    fn has_debug_info(&self) -> bool {
        false
    }

    fn debug_session(&self) -> Result<Self::Session, Self::Error> {
        Ok(NoDebugSession)
    }
}

pub(crate) fn arch_from_machine(machine: MachineType) -> Arch {
    match machine {
        MachineType::X86 => Arch::X86,
        MachineType::Amd64 => Arch::X86_64,
        MachineType::Arm => Arch::Arm,
        // TODO(ja): Add this when PR is merged
        // MachineType::Arm64 => Arch::Arm64,
        MachineType::PowerPC => Arch::Ppc,
        _ => Arch::Unknown,
    }
}

pub struct PdbSymbolIterator<'d, 'o> {
    symbols: pdb::SymbolIter<'o>,
    _ph: PhantomData<&'d ()>,
}

impl<'d, 'o> Iterator for PdbSymbolIterator<'d, 'o> {
    type Item = Symbol<'d>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Ok(Some(symbol)) = self.symbols.next() {
            if let Ok(pdb::SymbolData::PublicSymbol(public)) = symbol.parse() {
                if !public.function {
                    continue;
                }

                return Some(Symbol {
                    // TODO: pdb::SymbolIter offers data bound to its own lifetime.
                    // Thus, we cannot return zero-copy symbols here.
                    name: symbol
                        .name()
                        .ok()
                        .map(|n| n.to_string().into_owned().into()),
                    address: u64::from(public.offset),
                    size: 0, // Computed in `SymbolMap`
                });
            }
        }

        None
    }
}
