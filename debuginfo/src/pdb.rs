use std::borrow::Cow;
use std::fmt;
use std::io::Cursor;
use std::marker::PhantomData;

use failure::Fail;
use fallible_iterator::FallibleIterator;
use parking_lot::RwLock;
use pdb::{AddressTranslator, MachineType, SymbolData};

use symbolic_common::{Arch, AsSelf, DebugId, Uuid};

use crate::base::*;
use crate::private::{HexFmt, Parse};

type Pdb<'d> = pdb::PDB<'d, Cursor<&'d [u8]>>;

const MAGIC_BIG: &[u8] = b"Microsoft C/C++ MSF 7.00\r\n\x1a\x44\x53\x00\x00\x00";

#[derive(Debug, Fail)]
pub enum PdbError {
    #[fail(display = "invalid PDB file")]
    Pdb(#[fail(cause)] pdb::Error),
}

pub struct PdbObject<'d> {
    pdb: RwLock<Pdb<'d>>,
    debug_info: pdb::DebugInformation<'d>,
    pdb_info: pdb::PDBInformation<'d>,
    public_syms: pdb::SymbolTable<'d>,
    data: &'d [u8],
}

// NB: The pdb crate simulates mmap behavior on any Read + Seek type. This implementation requires
// mutability of the `Source` and uses trait objects without a Send + Sync barrier. We know that we
// only instanciate `&[u8]` as source. Whenever we mutate the reader (to read a new module stream),
// we acquire a write lock on the PDB, which should be sufficient.
unsafe impl Send for PdbObject<'_> {}
unsafe impl Sync for PdbObject<'_> {}

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
            pdb: RwLock::new(pdb),
            debug_info: dbi,
            pdb_info: pdbi,
            public_syms: pubi,
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
        // The PDB only stores relative addresses, so the load_address does not make sense. The
        // according PE, however does feature a load address (called `image_base`). See
        // `PeObject::load_address` for more information.
        0
    }

    pub fn has_symbols(&self) -> bool {
        // We can safely assume that PDBs will always contain symbols.
        true
    }

    pub fn symbols(&self) -> PdbSymbolIterator<'d, '_> {
        PdbSymbolIterator {
            symbols: self.public_syms.iter(),
            // TODO: Only compute this once and cache it internally.
            translator: self.pdb.write().address_translator().ok(),
        }
    }

    pub fn symbol_map(&self) -> SymbolMap<'d> {
        self.symbols().collect()
    }

    pub fn has_debug_info(&self) -> bool {
        false // TODO(ja): Implement
    }

    pub fn debug_session(&self) -> Result<PdbDebugSession<'d>, PdbError> {
        Ok(PdbDebugSession { _ph: PhantomData })
    }

    pub fn has_unwind_info(&self) -> bool {
        false // TODO(ja): Implement
    }

    pub fn data(&self) -> &'d [u8] {
        self.data
    }
}

impl fmt::Debug for PdbObject<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("PdbObject")
            .field("id", &self.id())
            .field("arch", &self.arch())
            .field("load_address", &HexFmt(self.load_address()))
            .field("has_symbols", &self.has_symbols())
            .field("has_debug_info", &self.has_debug_info())
            .field("has_unwind_info", &self.has_unwind_info())
            .finish()
    }
}

impl<'d, 'slf: 'd> AsSelf<'slf> for PdbObject<'d> {
    type Ref = PdbObject<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        self
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

impl<'d> ObjectLike for PdbObject<'d> {
    type Error = PdbError;
    type Session = PdbDebugSession<'d>;

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

    fn has_debug_info(&self) -> bool {
        self.has_debug_info()
    }

    fn debug_session(&self) -> Result<Self::Session, Self::Error> {
        self.debug_session()
    }

    fn has_unwind_info(&self) -> bool {
        self.has_unwind_info()
    }
}

pub(crate) fn arch_from_machine(machine: MachineType) -> Arch {
    match machine {
        MachineType::X86 => Arch::X86,
        MachineType::Amd64 => Arch::Amd64,
        MachineType::Arm => Arch::Arm,
        // TODO(ja): Add this when PR is merged
        // MachineType::Arm64 => Arch::Arm64,
        MachineType::PowerPC => Arch::Ppc,
        _ => Arch::Unknown,
    }
}

pub struct PdbSymbolIterator<'d, 'o> {
    symbols: pdb::SymbolIter<'o>,
    translator: Option<AddressTranslator<'d>>,
}

impl<'d, 'o> Iterator for PdbSymbolIterator<'d, 'o> {
    type Item = Symbol<'d>;

    fn next(&mut self) -> Option<Self::Item> {
        let translator = self.translator.as_ref()?;

        while let Ok(Some(symbol)) = self.symbols.next() {
            if let Ok(SymbolData::PublicSymbol(public)) = symbol.parse() {
                if !public.function {
                    continue;
                }

                // The RVA translation might yield zero, which in this case will most likely refer
                // to a missing section or invalid symbol. Silently skip this case.
                let address = public.rva(translator);
                if address == 0 {
                    continue;
                }

                let name = symbol.name().ok().map(|name| {
                    let cow = name.to_string();
                    // TODO: pdb::SymbolIter offers data bound to its own lifetime.
                    // Thus, we cannot return zero-copy symbols here.
                    Cow::from(String::from(if cow.starts_with('_') {
                        &cow[1..]
                    } else {
                        &cow
                    }))
                });

                return Some(Symbol {
                    name,
                    address: u64::from(address),
                    size: 0, // Computed in `SymbolMap`
                });
            }
        }

        None
    }
}

#[derive(Debug)]
pub struct PdbDebugSession<'d> {
    _ph: PhantomData<&'d ()>,
}

impl DebugSession for PdbDebugSession<'_> {
    type Error = PdbError;

    fn functions(&mut self) -> Result<Vec<Function<'_>>, PdbError> {
        Ok(Vec::new())
    }
}

impl<'d, 'slf: 'd> AsSelf<'slf> for PdbDebugSession<'d> {
    type Ref = PdbDebugSession<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        self
    }
}
