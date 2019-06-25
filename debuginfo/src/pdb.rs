//! Support for Program Database, the debug companion format on Windows.

use std::borrow::Cow;
use std::fmt;
use std::io::Cursor;
use std::sync::Arc;

use failure::Fail;
use lazycell::LazyCell;
use parking_lot::RwLock;
use pdb::{AddressMap, FallibleIterator, MachineType, Module, ModuleInfo, SymbolData};

use symbolic_common::{
    derive_failure, split_path_bytes, Arch, AsSelf, CodeId, CpuFamily, DebugId, Name, SelfCell,
    Uuid,
};

use crate::base::*;
use crate::private::Parse;

type Pdb<'d> = pdb::PDB<'d, Cursor<&'d [u8]>>;

const MAGIC_BIG: &[u8] = b"Microsoft C/C++ MSF 7.00\r\n\x1a\x44\x53\x00\x00\x00";

// Used for CFI, remove once abstraction is complete
#[doc(hidden)]
pub use pdb;

/// Variants of [`PdbError`](struct.PdbError.html).
#[derive(Clone, Copy, Debug, Eq, Fail, PartialEq)]
pub enum PdbErrorKind {
    /// The PDB file is corrupted. See the cause for more information.
    #[fail(display = "invalid pdb file")]
    BadObject,
}

derive_failure!(
    PdbError,
    PdbErrorKind,
    doc = "An error when dealing with [`PdbObject`](struct.PdbObject.html)."
);

impl From<pdb::Error> for PdbError {
    fn from(error: pdb::Error) -> Self {
        error.context(PdbErrorKind::BadObject).into()
    }
}

/// Program Database, the debug companion format on Windows.
///
/// This object is a sole debug companion to [`PeObject`](../pdb/struct.PdbObject.html).
pub struct PdbObject<'d> {
    pdb: Arc<RwLock<Pdb<'d>>>,
    debug_info: Arc<pdb::DebugInformation<'d>>,
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
        let mut pdb = Pdb::open(Cursor::new(data))?;
        let dbi = pdb.debug_information()?;
        let pdbi = pdb.pdb_information()?;
        let pubi = pdb.global_symbols()?;

        Ok(PdbObject {
            pdb: Arc::new(RwLock::new(pdb)),
            debug_info: Arc::new(dbi),
            pdb_info: pdbi,
            public_syms: pubi,
            data,
        })
    }

    /// The container file format, which is always `FileFormat::Pdb`.
    pub fn file_format(&self) -> FileFormat {
        FileFormat::Pdb
    }

    /// The code identifier of this object, always `None`.
    ///
    /// PDB files do not contain sufficient information to compute the code identifier, since they
    /// are lacking the relevant parts of the PE header.
    pub fn code_id(&self) -> Option<CodeId> {
        None
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
        // Prefer the age from the debug information stream, as it is more likely to correspond to
        // the executable than the PDB info header. The latter is often bumped independently when
        // the PDB is processed or optimized, which causes it to go out of sync with the original
        // image.
        let age = self.debug_info.age().unwrap_or(self.pdb_info.age);
        match Uuid::from_slice(&self.pdb_info.guid.as_bytes()[..]) {
            Ok(uuid) => DebugId::from_parts(uuid, age),
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

    /// Returns an iterator over symbols in the public symbol table.
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
        // There is no cheap way to find out if a PDB contains debugging information that we care
        // about. Effectively, we're interested in local symbols declared in the module info
        // streams. To reliably determine whether any stream is present, we'd have to probe each one
        // of them, which can result in quite a lot of disk I/O.
        true
    }

    /// Determines whether this object contains embedded source.
    pub fn has_source(&self) -> bool {
        false
    }

    /// Constructs a debugging session.
    pub fn debug_session(&self) -> Result<PdbDebugSession<'d>, PdbError> {
        PdbDebugSession::build(self, self.debug_info.clone())
    }

    /// Determines whether this object contains stack unwinding information.
    pub fn has_unwind_info(&self) -> bool {
        // The PDB crate currently loads quite a lot of information from the PDB when accessing the
        // frame table. However, we expect unwind info in every PDB for 32-bit builds, so we can
        // just assume it's there if the architecture matches.
        // TODO: Implement a better way by exposing the extra streams in the PDB crate.
        self.arch().cpu_family() == CpuFamily::Intel32
    }

    /// Returns the raw data of the ELF file.
    pub fn data(&self) -> &'d [u8] {
        self.data
    }

    #[doc(hidden)]
    pub fn inner(&self) -> &RwLock<Pdb<'d>> {
        &self.pdb
    }
}

impl fmt::Debug for PdbObject<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("PdbObject")
            .field("debug_id", &self.debug_id())
            .field("arch", &self.arch())
            .field("load_address", &format_args!("{:#x}", self.load_address()))
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

    fn has_source(&self) -> bool {
        self.has_source()
    }
}

pub(crate) fn arch_from_machine(machine: MachineType) -> Arch {
    match machine {
        MachineType::X86 => Arch::X86,
        MachineType::Amd64 => Arch::Amd64,
        MachineType::Arm => Arch::Arm,
        MachineType::Arm64 => Arch::Arm64,
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
                    // pdb::SymbolIter offers data bound to its own lifetime since it holds the
                    // buffer containing public symbols. The contract requires that we return
                    // `Symbol<'d>`, so we cannot return zero-copy symbols here.
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

struct PdbDebugInfo<'d> {
    pdb: Arc<RwLock<Pdb<'d>>>,
    modules: Vec<Module<'d>>,
    module_infos: Vec<LazyCell<Option<ModuleInfo<'d>>>>,
    address_map: pdb::AddressMap<'d>,
    string_table: Option<pdb::StringTable<'d>>,
    symbol_map: SymbolMap<'d>,
}

impl<'d> PdbDebugInfo<'d> {
    fn build(
        pdb: &PdbObject<'d>,
        debug_info: &'d pdb::DebugInformation<'d>,
    ) -> Result<Self, PdbError> {
        let symbol_map = pdb.symbol_map();
        let modules = debug_info.modules()?.collect::<Vec<_>>()?;
        let module_infos = modules.iter().map(|_| LazyCell::new()).collect();

        // Avoid deadlocks by only covering the two access to the address map and string table. For
        // instance, `pdb.symbol_map()` requires a mutable borrow of the PDB as well.
        let mut p = pdb.pdb.write();
        let address_map = p.address_map()?;

        // PDB::string_table errors if the named stream for the string table is not present.
        // However, this occurs in certain PDBs and does not automatically indicate an error.
        let string_table = match p.string_table() {
            Ok(string_table) => Some(string_table),
            Err(pdb::Error::StreamNameNotFound) => None,
            Err(e) => return Err(e.into()),
        };

        drop(p);

        Ok(PdbDebugInfo {
            pdb: pdb.pdb.clone(),
            modules,
            module_infos,
            address_map,
            string_table,
            symbol_map,
        })
    }

    /// Returns an iterator over all compilation units (modules).
    fn units(&'d self) -> PdbUnitIterator<'_> {
        PdbUnitIterator {
            debug_info: self,
            index: 0,
        }
    }

    fn get_module(&'d self, index: usize) -> Result<Option<&ModuleInfo<'_>>, PdbError> {
        // Silently ignore module references out-of-bound
        let cell = match self.module_infos.get(index) {
            Some(cell) => cell,
            None => return Ok(None),
        };

        let module_opt = cell.try_borrow_with(|| {
            let module = &self.modules[index];
            self.pdb.write().module_info(module)
        })?;

        Ok(module_opt.as_ref())
    }
}

impl<'slf, 'd: 'slf> AsSelf<'slf> for PdbDebugInfo<'d> {
    type Ref = PdbDebugInfo<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        unsafe { std::mem::transmute(self) }
    }
}

/// Debug session for PDB objects.
pub struct PdbDebugSession<'d> {
    cell: SelfCell<Arc<pdb::DebugInformation<'d>>, PdbDebugInfo<'d>>,
}

impl<'d> PdbDebugSession<'d> {
    fn build(
        pdb: &PdbObject<'d>,
        debug_info: Arc<pdb::DebugInformation<'d>>,
    ) -> Result<Self, PdbError> {
        let cell = SelfCell::try_new(debug_info, |debug_info| {
            PdbDebugInfo::build(pdb, unsafe { &*debug_info })
        })?;

        Ok(PdbDebugSession { cell })
    }

    /// Returns an iterator over all functions in this debug file.
    pub fn functions(&mut self) -> PdbFunctionIterator<'_> {
        PdbFunctionIterator {
            units: self.cell.get().units(),
            functions: Vec::new().into_iter(),
            finished: false,
        }
    }
}

impl DebugSession for PdbDebugSession<'_> {
    type Error = PdbError;

    fn functions(&mut self) -> DynIterator<'_, Result<Function<'_>, Self::Error>> {
        Box::new(self.functions())
    }
}

struct Unit<'s> {
    debug_info: &'s PdbDebugInfo<'s>,
    module: &'s pdb::ModuleInfo<'s>,
}

impl<'s> Unit<'s> {
    fn functions(&self) -> Result<Vec<Function<'s>>, PdbError> {
        let address_map = &self.debug_info.address_map;
        let string_table = &self.debug_info.string_table;
        let symbol_map = &self.debug_info.symbol_map;

        let program = self.module.line_program()?;
        let mut symbols = self.module.symbols()?;

        let mut functions = Vec::new();
        while let Some(symbol) = symbols.next()? {
            let proc = match symbol.parse() {
                Ok(SymbolData::Procedure(proc)) => proc,
                // We need to ignore errors here since the PDB crate does not yet implement all
                // symbol types. Instead of erroring too often, it's better to swallow these.
                _ => continue,
            };

            // Translate the function's address to the PE's address space. If this fails, we're
            // likely dealing with an invalid function and can skip it.
            let address = match proc.offset.to_rva(&address_map) {
                Some(addr) => u64::from(addr.0),
                None => continue,
            };

            // Prefer names from the public symbol table as they are mangled. Otherwise, fall back
            // to the name of the private symbol which is often times demangled. Also, this might
            // save us some allocations, since the symbol_map is held by the debug session.
            let name = match symbol_map.lookup(address) {
                Some(symbol) => Name::new(symbol.name().unwrap_or_default()),
                None => Name::new(symbol.name()?.to_string()),
            };

            let mut lines = Vec::new();
            let mut line_iter = program.lines_at_offset(proc.offset);
            while let Some(line_info) = line_iter.next()? {
                let rva = match line_info.offset.to_rva(&address_map) {
                    Some(rva) => u64::from(rva.0),
                    None => continue,
                };

                let file_info = program.get_file_info(line_info.file_index)?;
                let file_path = match string_table {
                    Some(string_table) => file_info.name.to_raw_string(&string_table)?,
                    None => "".into(),
                };
                let (dir, name) = split_path_bytes(file_path.as_bytes());

                lines.push(LineInfo {
                    address: rva,
                    file: FileInfo {
                        dir: dir.unwrap_or_default(),
                        name,
                    },
                    line: line_info.line_start.into(),
                });
            }

            let func = Function {
                address,
                size: proc.len.into(),
                name,
                compilation_dir: &[],
                lines,
                inlinees: Vec::new(),
                inline: false,
            };

            functions.push(func);
        }

        // Functions are not necessarily in RVA order. So far, it seems that modules are.
        dmsort::sort_by_key(&mut functions, |f| f.address);

        Ok(functions)
    }
}

struct PdbUnitIterator<'s> {
    debug_info: &'s PdbDebugInfo<'s>,
    index: usize,
}

impl<'s> Iterator for PdbUnitIterator<'s> {
    type Item = Result<Unit<'s>, PdbError>;

    fn next(&mut self) -> Option<Self::Item> {
        let debug_info = self.debug_info;
        while self.index < debug_info.modules.len() {
            let result = debug_info.get_module(self.index);
            self.index += 1;

            let module = match result {
                Ok(Some(module)) => module,
                Ok(None) => continue,
                Err(error) => return Some(Err(error)),
            };

            return Some(Ok(Unit { debug_info, module }));
        }

        None
    }
}

/// An iterator over functions in a PDB file.
pub struct PdbFunctionIterator<'s> {
    units: PdbUnitIterator<'s>,
    functions: std::vec::IntoIter<Function<'s>>,
    finished: bool,
}

impl<'s> Iterator for PdbFunctionIterator<'s> {
    type Item = Result<Function<'s>, PdbError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        loop {
            if let Some(func) = self.functions.next() {
                return Some(Ok(func));
            }

            let unit = match self.units.next() {
                Some(Ok(unit)) => unit,
                Some(Err(error)) => return Some(Err(error)),
                None => break,
            };

            self.functions = match unit.functions() {
                Ok(functions) => functions.into_iter(),
                Err(error) => return Some(Err(error)),
            };
        }

        self.finished = true;
        None
    }
}

impl std::iter::FusedIterator for PdbFunctionIterator<'_> {}
