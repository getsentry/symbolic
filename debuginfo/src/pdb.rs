//! Support for Program Database, the debug companion format on Windows.

use std::borrow::Cow;
use std::fmt;
use std::io::Cursor;
use std::marker::PhantomData;

use failure::Fail;
use fallible_iterator::FallibleIterator;
use parking_lot::RwLock;
use pdb::{AddressMap, MachineType, SymbolData};

use symbolic_common::{Arch, AsSelf, DebugId, Uuid};

use crate::base::*;
use crate::private::{HexFmt, Parse};

type Pdb<'d> = pdb::PDB<'d, Cursor<&'d [u8]>>;

const MAGIC_BIG: &[u8] = b"Microsoft C/C++ MSF 7.00\r\n\x1a\x44\x53\x00\x00\x00";

/// An error when dealing with [`PdbObject`](struct.PdbObject.html).
#[derive(Debug, Fail)]
pub enum PdbError {
    /// The data in the PDB file could not be parsed.
    #[fail(display = "invalid PDB file")]
    BadObject(#[fail(cause)] pdb::Error),
}

/// Program Database, the debug companion format on Windows.
///
/// This object is a sole debug companion to [`PeObject`](../pdb/struct.PdbObject.html).
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
    /// Tests whether the buffer could contain an PDB object.
    pub fn test(data: &[u8]) -> bool {
        // NB: "Microsoft C/C++ program database 2.00" is not supported by the pdb crate, so there
        // is no point in pretending we could read it.
        data.starts_with(MAGIC_BIG)
    }

    /// Tries to parse a PDB object from the given slice.
    pub fn parse(data: &'d [u8]) -> Result<Self, PdbError> {
        let mut pdb = Pdb::open(Cursor::new(data)).map_err(PdbError::BadObject)?;
        let dbi = pdb.debug_information().map_err(PdbError::BadObject)?;
        let pdbi = pdb.pdb_information().map_err(PdbError::BadObject)?;
        let pubi = pdb.global_symbols().map_err(PdbError::BadObject)?;

        Ok(PdbObject {
            pdb: RwLock::new(pdb),
            debug_info: dbi,
            pdb_info: pdbi,
            public_syms: pubi,
            data,
        })
    }

    /// The container file format, which is always `FileFormat::Pdb`.
    pub fn file_format(&self) -> FileFormat {
        FileFormat::Pdb
    }

    /// The debug information identifier of this PDB.
    ///
    /// The PDB stores a specific header that contains GUID and age bits. Additionally, Microsoft
    /// uses the file name of the PDB to avoid GUID collisions. In most contexts, however, it is
    /// sufficient to rely on the uniqueness of the GUID to identify a PDB.
    ///
    /// The same information is also stored in a header in the corresponding PE file, which can be
    /// used to locate a PDB from a PE.
    pub fn debug_id(&self) -> DebugId {
        match Uuid::from_slice(&self.pdb_info.guid.as_bytes()[..]) {
            Ok(uuid) => DebugId::from_parts(uuid, self.pdb_info.age),
            Err(_) => DebugId::default(),
        }
    }

    /// The CPU architecture of this object, as specified in the debug information stream (DBI).
    pub fn arch(&self) -> Arch {
        self.debug_info
            .machine_type()
            .ok()
            .map(arch_from_machine)
            .unwrap_or_default()
    }

    /// The kind of this object, which is always `Debug`.
    pub fn kind(&self) -> ObjectKind {
        ObjectKind::Debug
    }

    /// The address at which the image prefers to be loaded into memory.
    ///
    /// The PDB only stores relative addresses, and more importantly, does not provide sufficient
    /// information to compute the original PE's load address. The according PE, however does
    /// feature a load address (called `image_base`). See [`PeObject::load_address`] for more
    /// information.
    ///
    /// [`PeObject::load_address`]: ../pe/struct.PeObject.html#method.load_address
    pub fn load_address(&self) -> u64 {
        0
    }

    /// Determines whether this object exposes a public symbol table.
    pub fn has_symbols(&self) -> bool {
        // We can safely assume that PDBs will always contain symbols.
        true
    }

    /// Returns an iterator over symbols of a public symbol table.
    pub fn symbols(&self) -> PdbSymbolIterator<'d, '_> {
        PdbSymbolIterator {
            symbols: self.public_syms.iter(),
            address_map: self.pdb.write().address_map().ok(),
        }
    }

    /// Returns an ordered map of symbols in the symbol table.
    pub fn symbol_map(&self) -> SymbolMap<'d> {
        self.symbols().collect()
    }

    /// Determines whether this object contains debug information.
    pub fn has_debug_info(&self) -> bool {
        false // TODO(ja): Implement
    }

    /// Constructs a debugging session.
    pub fn debug_session(&self) -> Result<PdbDebugSession<'d>, PdbError> {
        Ok(PdbDebugSession { _ph: PhantomData })
    }

    /// Determines whether this object contains stack unwinding information.
    pub fn has_unwind_info(&self) -> bool {
        false // TODO(ja): Implement
    }

    /// Returns the raw data of the ELF file.
    pub fn data(&self) -> &'d [u8] {
        self.data
    }
}

impl fmt::Debug for PdbObject<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("PdbObject")
            .field("debug_id", &self.debug_id())
            .field("arch", &self.arch())
            .field("load_address", &HexFmt(self.load_address()))
            .field("has_symbols", &self.has_symbols())
            .field("has_debug_info", &self.has_debug_info())
            .field("has_unwind_info", &self.has_unwind_info())
            .finish()
    }
}

impl<'slf, 'd: 'slf> AsSelf<'slf> for PdbObject<'d> {
    type Ref = PdbObject<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        unsafe { std::mem::transmute(self) }
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

/// An iterator over symbols in the PDB file.
///
/// Returned by [`PdbObject::symbols`](struct.PdbObject.html#method.symbols).
pub struct PdbSymbolIterator<'d, 'o> {
    symbols: pdb::SymbolIter<'o>,
    address_map: Option<AddressMap<'d>>,
}

impl<'d, 'o> Iterator for PdbSymbolIterator<'d, 'o> {
    type Item = Symbol<'d>;

    fn next(&mut self) -> Option<Self::Item> {
        let address_map = self.address_map.as_ref()?;

        while let Ok(Some(symbol)) = self.symbols.next() {
            if let Ok(SymbolData::PublicSymbol(public)) = symbol.parse() {
                if !public.function {
                    continue;
                }

                let address = match public.offset.to_rva(address_map) {
                    Some(address) => address,
                    None => continue,
                };

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
                    address: u64::from(address.0),
                    size: 0, // Computed in `SymbolMap`
                });
            }
        }

        None
    }
}

/// Debug session for PDB objects.
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

impl<'slf, 'd: 'slf> AsSelf<'slf> for PdbDebugSession<'d> {
    type Ref = PdbDebugSession<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        self
    }
}
