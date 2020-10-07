//! Support for Program Database, the debug companion format on Windows.

use std::borrow::Cow;
use std::cell::{RefCell, RefMut};
use std::cmp::Ordering;
use std::collections::btree_map::{BTreeMap, Entry};
use std::fmt;
use std::io::Cursor;
use std::sync::Arc;

use lazycell::LazyCell;
use parking_lot::RwLock;
use pdb::{
    AddressMap, FallibleIterator, InlineSiteSymbol, ItemIndex, LineProgram, MachineType, Module,
    ModuleInfo, PdbInternalSectionOffset, ProcedureSymbol, SymbolData,
};
use smallvec::SmallVec;
use thiserror::Error;

use symbolic_common::{Arch, AsSelf, CodeId, CpuFamily, DebugId, Name, SelfCell, Uuid};

use crate::base::*;
use crate::private::{FunctionStack, Parse};

type Pdb<'d> = pdb::PDB<'d, Cursor<&'d [u8]>>;

const MAGIC_BIG: &[u8] = b"Microsoft C/C++ MSF 7.00\r\n\x1a\x44\x53\x00\x00\x00";

// Used for CFI, remove once abstraction is complete
#[doc(hidden)]
pub use pdb;

/// An error when dealing with [`PdbObject`](struct.PdbObject.html).
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum PdbError {
    /// The PDB file is corrupted. See the cause for more information.
    #[error("invalid pdb file")]
    BadObject(#[from] pdb::Error),

    /// An inline record was encountered without an inlining parent.
    #[error("unexpected inline function without parent")]
    UnexpectedInline,

    /// Formatting of a type name failed
    #[error("failed to format type name")]
    FormattingFailed(#[from] fmt::Error),
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
    pub fn has_sources(&self) -> bool {
        false
    }

    /// Constructs a debugging session.
    pub fn debug_session(&self) -> Result<PdbDebugSession<'d>, PdbError> {
        PdbDebugSession::build(self)
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

impl<'d: 'slf, 'slf: 'sess, 'sess> ObjectLike<'d, 'slf, 'sess> for PdbObject<'d> {
    type Error = PdbError;
    type Session = PdbDebugSession<'d>;
    type SymbolIterator = PdbSymbolIterator<'d, 'slf>;

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

    fn symbols(&'slf self) -> Self::SymbolIterator {
        self.symbols()
    }

    fn symbol_map(&self) -> SymbolMap<'d> {
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
            if let Ok(SymbolData::Public(public)) = symbol.parse() {
                if !public.function {
                    continue;
                }

                let address = match public.offset.to_rva(address_map) {
                    Some(address) => address,
                    None => continue,
                };

                let cow = public.name.to_string();
                // pdb::SymbolIter offers data bound to its own lifetime since it holds the
                // buffer containing public symbols. The contract requires that we return
                // `Symbol<'d>`, so we cannot return zero-copy symbols here.
                let name = Cow::from(String::from(if cow.starts_with('_') {
                    &cow[1..]
                } else {
                    &cow
                }));

                return Some(Symbol {
                    name: Some(name),
                    address: u64::from(address.0),
                    size: 0, // Computed in `SymbolMap`
                });
            }
        }

        None
    }
}

struct ItemMap<'s, I: ItemIndex> {
    iter: pdb::ItemIter<'s, I>,
    finder: pdb::ItemFinder<'s, I>,
}

impl<'s, I> ItemMap<'s, I>
where
    I: ItemIndex,
{
    pub fn try_get(&mut self, index: I) -> Result<pdb::Item<'s, I>, PdbError> {
        if index <= self.finder.max_index() {
            return Ok(self.finder.find(index)?);
        }

        while let Some(item) = self.iter.next()? {
            self.finder.update(&self.iter);
            match item.index().partial_cmp(&index) {
                Some(Ordering::Equal) => return Ok(item),
                Some(Ordering::Greater) => break,
                _ => continue,
            }
        }

        Err(pdb::Error::TypeNotFound(index.into()).into())
    }
}

type TypeMap<'d> = ItemMap<'d, pdb::TypeIndex>;
type IdMap<'d> = ItemMap<'d, pdb::IdIndex>;

struct PdbStreams<'d> {
    debug_info: Arc<pdb::DebugInformation<'d>>,
    type_info: pdb::TypeInformation<'d>,
    id_info: pdb::IdInformation<'d>,
}

impl<'d> PdbStreams<'d> {
    fn from_pdb(pdb: &PdbObject<'d>) -> Result<Self, PdbError> {
        let mut p = pdb.pdb.write();

        Ok(Self {
            debug_info: pdb.debug_info.clone(),
            type_info: p.type_information()?,
            id_info: p.id_information()?,
        })
    }

    fn type_map(&self) -> TypeMap<'_> {
        ItemMap {
            iter: self.type_info.iter(),
            finder: self.type_info.finder(),
        }
    }

    fn id_map(&self) -> IdMap<'_> {
        ItemMap {
            iter: self.id_info.iter(),
            finder: self.id_info.finder(),
        }
    }
}

struct PdbDebugInfo<'d> {
    /// The original PDB to load module streams on demand.
    pdb: Arc<RwLock<Pdb<'d>>>,
    /// All module headers for repeated iteration.
    modules: Vec<Module<'d>>,
    /// Lazy loaded module streams in the same order as headers.
    module_infos: Vec<LazyCell<Option<ModuleInfo<'d>>>>,
    /// Cache for module by name lookup for cross module imports.
    module_exports: RefCell<BTreeMap<pdb::ModuleRef, Option<pdb::CrossModuleExports>>>,
    /// OMAP structure to map reordered sections to RVAs.
    address_map: pdb::AddressMap<'d>,
    /// String table for name lookups.
    string_table: Option<pdb::StringTable<'d>>,
    /// Lazy loaded map of the TPI stream.
    type_map: RefCell<TypeMap<'d>>,
    /// Lazy loaded map of the IPI stream.
    id_map: RefCell<IdMap<'d>>,
}

impl<'d> PdbDebugInfo<'d> {
    fn build(pdb: &PdbObject<'d>, streams: &'d PdbStreams<'d>) -> Result<Self, PdbError> {
        let modules = streams.debug_info.modules()?.collect::<Vec<_>>()?;
        let module_infos = modules.iter().map(|_| LazyCell::new()).collect();
        let module_exports = RefCell::new(BTreeMap::new());
        let type_map = RefCell::new(streams.type_map());
        let id_map = RefCell::new(streams.id_map());

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
            module_exports,
            address_map,
            string_table,
            type_map,
            id_map,
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

    fn file_info(&self, file_info: pdb::FileInfo<'d>) -> Result<FileInfo<'_>, PdbError> {
        let file_path = match self.string_table {
            Some(ref string_table) => file_info.name.to_raw_string(string_table)?,
            None => "".into(),
        };

        Ok(FileInfo::from_path(file_path.as_bytes()))
    }

    fn get_exports(
        &'d self,
        module_ref: pdb::ModuleRef,
    ) -> Result<Option<pdb::CrossModuleExports>, PdbError> {
        let name = match self.string_table {
            Some(ref string_table) => module_ref.0.to_string_lossy(string_table)?,
            None => return Ok(None),
        };

        let module_index = self
            .modules
            .iter()
            .position(|m| m.module_name().eq_ignore_ascii_case(&name));

        let module = match module_index {
            Some(index) => self.get_module(index)?,
            None => None,
        };

        Ok(match module {
            Some(module) => Some(module.exports()?),
            None => None,
        })
    }

    fn resolve_import<I: ItemIndex>(
        &'d self,
        cross_ref: pdb::CrossModuleRef<I>,
    ) -> Result<Option<I>, PdbError> {
        let pdb::CrossModuleRef(module_ref, local_index) = cross_ref;

        let mut module_exports = self.module_exports.borrow_mut();
        let exports = match module_exports.entry(module_ref) {
            Entry::Vacant(vacant) => vacant.insert(self.get_exports(module_ref)?),
            Entry::Occupied(occupied) => occupied.into_mut(),
        };

        Ok(if let Some(ref exports) = *exports {
            exports.resolve_import(local_index)?
        } else {
            None
        })
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
    cell: SelfCell<Box<PdbStreams<'d>>, PdbDebugInfo<'d>>,
}

impl<'d> PdbDebugSession<'d> {
    fn build(pdb: &PdbObject<'d>) -> Result<Self, PdbError> {
        let streams = PdbStreams::from_pdb(pdb)?;
        let cell = SelfCell::try_new(Box::new(streams), |streams| {
            PdbDebugInfo::build(pdb, unsafe { &*streams })
        })?;

        Ok(PdbDebugSession { cell })
    }

    /// Returns an iterator over all source files in this debug file.
    pub fn files(&self) -> PdbFileIterator<'_> {
        PdbFileIterator {
            debug_info: self.cell.get(),
            units: self.cell.get().units(),
            files: pdb::FileIterator::default(),
            finished: false,
        }
    }

    /// Returns an iterator over all functions in this debug file.
    pub fn functions(&self) -> PdbFunctionIterator<'_> {
        PdbFunctionIterator {
            units: self.cell.get().units(),
            functions: Vec::new().into_iter(),
            finished: false,
        }
    }

    /// Looks up a file's source contents by its full canonicalized path.
    ///
    /// The given path must be canonicalized.
    pub fn source_by_path(&self, _path: &str) -> Result<Option<Cow<'_, str>>, PdbError> {
        Ok(None)
    }
}

impl<'d: 'slf, 'slf> DebugSession<'d, 'slf> for PdbDebugSession<'d> {
    type Error = PdbError;
    type FunctionIterator = PdbFunctionIterator<'d>;
    type FileIterator = PdbFileIterator<'d>;

    fn functions(&'slf self) -> Self::FunctionIterator {
        self.functions()
    }

    fn files(&'slf self) -> Self::FileIterator {
        self.files()
    }

    fn source_by_path(&self, path: &str) -> Result<Option<Cow<'_, str>>, Self::Error> {
        self.source_by_path(path)
    }
}

/// Checks whether the given name declares an anonymous namespace.
///
/// ID records specify the mangled format for anonymous namespaces: `?A0x<id>`, where `id` is a hex
/// identifier of the namespace. Demanglers usually resolve this as "anonymous namespace".
fn is_anonymous_namespace(name: &str) -> bool {
    name.strip_prefix("?A0x")
        .map_or(false, |rest| u32::from_str_radix(rest, 16).is_ok())
}

/// Formatter for function types.
///
/// This formatter currently only contains the minimum implementation requried to format inline
/// function names without parameters.
struct TypeFormatter<'u, 'd> {
    unit: &'u Unit<'d>,
    type_map: RefMut<'u, TypeMap<'d>>,
    id_map: RefMut<'u, IdMap<'d>>,
}

impl<'u, 'd> TypeFormatter<'u, 'd> {
    /// Creates a new `TypeFormatter`.
    pub fn new(unit: &'u Unit<'d>) -> Self {
        Self {
            unit,
            type_map: unit.debug_info.type_map.borrow_mut(),
            id_map: unit.debug_info.id_map.borrow_mut(),
        }
    }

    /// Writes the `Id` with the given index.
    pub fn write_id<W: fmt::Write>(
        &mut self,
        target: &mut W,
        index: pdb::IdIndex,
    ) -> Result<(), PdbError> {
        let index = match self.unit.resolve_index(index)? {
            Some(index) => index,
            None => return Ok(write!(target, "<redacted>")?),
        };

        let id = self.id_map.try_get(index)?;
        match id.parse() {
            Ok(pdb::IdData::Function(data)) => {
                if let Some(scope) = data.scope {
                    self.write_id(target, scope)?;
                    write!(target, "::")?;
                }

                write!(target, "{}", data.name.to_string())?;
            }
            Ok(pdb::IdData::MemberFunction(data)) => {
                self.write_type(target, data.parent)?;
                write!(target, "::{}", data.name.to_string())?;
            }
            Ok(pdb::IdData::BuildInfo(_)) => {
                // nothing to do
            }
            Ok(pdb::IdData::StringList(data)) => {
                write!(target, "\"")?;
                for (i, string_index) in data.substrings.iter().enumerate() {
                    if i > 0 {
                        write!(target, "\" \"")?;
                    }
                    self.write_type(target, *string_index)?;
                }
                write!(target, "\"")?;
            }
            Ok(pdb::IdData::String(data)) => {
                let mut string = data.name.to_string();

                if is_anonymous_namespace(&string) {
                    string = Cow::Borrowed("`anonymous namespace'");
                }

                write!(target, "{}", string)?;
            }
            Ok(pdb::IdData::UserDefinedTypeSource(_)) => {
                // nothing to do.
            }
            Err(pdb::Error::UnimplementedTypeKind(_)) => {
                write!(target, "<unknown>")?;
            }
            Err(e) => return Err(e.into()),
        }

        Ok(())
    }

    /// Writes the `Type` with the given index.
    pub fn write_type<W: fmt::Write>(
        &mut self,
        target: &mut W,
        index: pdb::TypeIndex,
    ) -> Result<(), PdbError> {
        let index = match self.unit.resolve_index(index)? {
            Some(index) => index,
            None => return Ok(write!(target, "<redacted>")?),
        };

        let ty = self.type_map.try_get(index)?;
        match ty.parse() {
            Ok(pdb::TypeData::Primitive(_)) => {
                // nothing to do
            }
            Ok(pdb::TypeData::Class(data)) => {
                write!(target, "{}", data.name.to_string())?;
            }
            Ok(pdb::TypeData::Member(_)) => {
                // nothing to do
            }
            Ok(pdb::TypeData::MemberFunction(data)) => {
                self.write_type(target, data.return_type)?;
                write!(target, " ")?;
                self.write_type(target, data.class_type)?;
                write!(target, "::")?;
                self.write_type(target, data.argument_list)?;
            }
            Ok(pdb::TypeData::OverloadedMethod(_)) => {
                // nothing to do
            }
            Ok(pdb::TypeData::Method(_)) => {
                // nothing to do
            }
            Ok(pdb::TypeData::StaticMember(_)) => {
                // nothing to do
            }
            Ok(pdb::TypeData::Nested(_)) => {
                // nothing to do
            }
            Ok(pdb::TypeData::BaseClass(_)) => {
                // nothing to do
            }
            Ok(pdb::TypeData::VirtualBaseClass(_)) => {
                // nothing to do
            }
            Ok(pdb::TypeData::VirtualFunctionTablePointer(_)) => {
                // nothing to do
            }
            Ok(pdb::TypeData::Procedure(data)) => {
                match data.return_type {
                    Some(return_type) => self.write_type(target, return_type)?,
                    None => write!(target, "void")?,
                }

                write!(target, " ")?;
                self.write_type(target, data.argument_list)?;
            }
            Ok(pdb::TypeData::Pointer(data)) => {
                self.write_type(target, data.underlying_type)?;

                if let Some(containing_class) = data.containing_class {
                    write!(target, " ")?;
                    self.write_type(target, containing_class)?;
                } else {
                    match data.attributes.pointer_mode() {
                        pdb::PointerMode::Pointer => write!(target, "*")?,
                        pdb::PointerMode::LValueReference => write!(target, "&")?,
                        pdb::PointerMode::RValueReference => write!(target, "&&")?,
                        _ => (),
                    }

                    if data.attributes.is_const() {
                        write!(target, " const")?;
                    }
                    if data.attributes.is_volatile() {
                        write!(target, " volatile")?;
                    }
                    if data.attributes.is_unaligned() {
                        write!(target, " __unaligned")?;
                    }
                    if data.attributes.is_restrict() {
                        write!(target, " __restrict")?;
                    }
                }
            }
            Ok(pdb::TypeData::Modifier(data)) => {
                if data.constant {
                    write!(target, "const ")?;
                }
                if data.volatile {
                    write!(target, "volatile ")?;
                }
                if data.unaligned {
                    write!(target, "__unaligned ")?;
                }

                self.write_type(target, data.underlying_type)?;
            }
            Ok(pdb::TypeData::Enumeration(data)) => {
                write!(target, "{}", data.name.to_string())?;
            }
            Ok(pdb::TypeData::Enumerate(data)) => {
                write!(target, "{}", data.name.to_string())?;
            }
            Ok(pdb::TypeData::Array(_)) => {
                // nothing to do
            }
            Ok(pdb::TypeData::Union(data)) => {
                write!(target, "{}", data.name.to_string())?;
            }
            Ok(pdb::TypeData::Bitfield(_)) => {
                // nothing to do
            }
            Ok(pdb::TypeData::FieldList(_)) => {
                write!(target, "<field list>")?;
            }
            Ok(pdb::TypeData::ArgumentList(data)) => {
                write!(target, "(")?;
                for (i, arg_index) in data.arguments.iter().enumerate() {
                    if i > 0 {
                        write!(target, ", ")?;
                    }
                    self.write_type(target, *arg_index)?;
                }
                write!(target, ")")?;
            }
            Ok(pdb::TypeData::MethodList(_)) => {
                // nothing to do
            }
            Err(pdb::Error::UnimplementedTypeKind(_)) => {
                write!(target, "<unknown>")?;
            }
            Err(e) => return Err(e.into()),
        }

        Ok(())
    }

    /// Formats the `Id` with the given index to a string.
    pub fn format_id(&mut self, index: pdb::IdIndex) -> Result<String, PdbError> {
        let mut string = String::new();
        self.write_id(&mut string, index)?;
        Ok(string)
    }
}

struct Unit<'s> {
    debug_info: &'s PdbDebugInfo<'s>,
    module: &'s pdb::ModuleInfo<'s>,
    imports: pdb::CrossModuleImports<'s>,
}

impl<'s> Unit<'s> {
    fn load(
        debug_info: &'s PdbDebugInfo<'s>,
        module: &'s pdb::ModuleInfo<'s>,
    ) -> Result<Self, PdbError> {
        let imports = module.imports()?;

        Ok(Self {
            debug_info,
            module,
            imports,
        })
    }

    fn resolve_index<I>(&self, index: I) -> Result<Option<I>, PdbError>
    where
        I: ItemIndex,
    {
        if index.is_cross_module() {
            let cross_ref = self.imports.resolve_import(index)?;
            self.debug_info.resolve_import(cross_ref)
        } else {
            Ok(Some(index))
        }
    }

    fn collect_lines<I>(
        &self,
        mut line_iter: I,
        program: &LineProgram<'s>,
    ) -> Result<Vec<LineInfo<'s>>, PdbError>
    where
        I: FallibleIterator<Item = pdb::LineInfo>,
        PdbError: From<I::Error>,
    {
        let address_map = &self.debug_info.address_map;

        let mut lines = Vec::new();
        while let Some(line_info) = line_iter.next()? {
            let rva = match line_info.offset.to_rva(&address_map) {
                Some(rva) => u64::from(rva.0),
                None => continue,
            };

            let file_info = program.get_file_info(line_info.file_index)?;

            lines.push(LineInfo {
                address: rva,
                size: line_info.length.map(u64::from),
                file: self.debug_info.file_info(file_info)?,
                line: line_info.line_start.into(),
            });
        }

        Ok(lines)
    }

    fn handle_procedure(
        &self,
        proc: ProcedureSymbol<'s>,
        program: &LineProgram<'s>,
    ) -> Result<Option<Function<'s>>, PdbError> {
        let address_map = &self.debug_info.address_map;

        // Translate the function's address to the PE's address space. If this fails, we're
        // likely dealing with an invalid function and can skip it.
        let address = match proc.offset.to_rva(&address_map) {
            Some(addr) => u64::from(addr.0),
            None => return Ok(None),
        };

        // Names from the private symbol table are generally demangled. They contain the path of the
        // scope and name of the function itself, including type parameters, but do not contain
        // parameter lists or return types. This is good enough for us at the moment.
        let name = Name::new(proc.name.to_string());

        let line_iter = program.lines_at_offset(proc.offset);
        let lines = self.collect_lines(line_iter, program)?;

        Ok(Some(Function {
            address,
            size: proc.len.into(),
            name,
            compilation_dir: &[],
            lines,
            inlinees: Vec::new(),
            inline: false,
        }))
    }

    fn handle_inlinee(
        &self,
        inline_site: InlineSiteSymbol<'s>,
        parent_offset: PdbInternalSectionOffset,
        inlinee: &pdb::Inlinee<'s>,
        program: &LineProgram<'s>,
    ) -> Result<Option<Function<'s>>, PdbError> {
        let line_iter = inlinee.lines(parent_offset, &inline_site);
        let lines = self.collect_lines(line_iter, program)?;

        // If there are no line records, skip this inline function completely. Apparently, it was
        // eliminated by the compiler, and cannot be hit by the program anymore. For `symbolic`,
        // such functions do not have any use.
        let start = match lines.iter().map(|line| line.address).min() {
            Some(address) => address,
            None => return Ok(None),
        };

        let end = match lines
            .iter()
            .map(|line| line.address + line.size.unwrap_or(1))
            .max()
        {
            Some(address) => address,
            None => return Ok(None),
        };

        let mut formatter = TypeFormatter::new(self);
        let name = Name::new(formatter.format_id(inline_site.inlinee)?);

        Ok(Some(Function {
            address: start,
            size: end - start,
            name,
            compilation_dir: &[],
            lines,
            inlinees: Vec::new(),
            inline: true,
        }))
    }

    fn functions(&self) -> Result<Vec<Function<'s>>, PdbError> {
        let program = self.module.line_program()?;
        let mut symbols = self.module.symbols()?;

        // Depending on the compiler version, the inlinee table might not be sorted. Since constant
        // search through inlinees is too slow (due to repeated parsing), but Inlinees are rather
        // small structures, it is relatively cheap to collect them into an in-memory index.
        let inlinees: BTreeMap<_, _> = self
            .module
            .inlinees()?
            .map(|i| Ok((i.index(), i)))
            .collect()?;

        let mut depth = 0;
        let mut inc_next = false;
        let mut skipped_depth = None;

        let mut functions = Vec::new();
        let mut stack = FunctionStack::new();
        let mut proc_offsets = SmallVec::<[_; 3]>::new();

        while let Some(symbol) = symbols.next()? {
            if inc_next {
                depth += 1;
            }

            inc_next = symbol.starts_scope();
            if symbol.ends_scope() {
                depth -= 1;

                if proc_offsets.last().map_or(false, |&(d, _)| d >= depth) {
                    proc_offsets.pop();
                }
            }

            // If we're navigating within a skipped function (see below), we can ignore this
            // entry completely. Otherwise, we've moved out of any skipped function and can
            // reset the stored depth.
            match skipped_depth {
                Some(skipped) if depth > skipped => continue,
                _ => skipped_depth = None,
            }

            // Flush all functions out that exceed the current iteration depth. Since we
            // encountered a symbol at this level, there will be no more inlinees to the
            // previous function at the same level or any of it's children.
            if symbol.ends_scope() {
                stack.flush(depth, &mut functions);
            }

            let function = match symbol.parse() {
                Ok(SymbolData::Procedure(proc)) => {
                    proc_offsets.push((depth, proc.offset));
                    self.handle_procedure(proc, &program)?
                }
                Ok(SymbolData::InlineSite(site)) => {
                    let parent_offset = proc_offsets
                        .last()
                        .map(|&(_, offset)| offset)
                        .ok_or(PdbError::UnexpectedInline)?;

                    // We can assume that inlinees will be listed in the inlinee table. If missing,
                    // skip silently instead of erroring out. Missing a single inline function is
                    // more acceptable in such a case than halting iteration completely.
                    if let Some(inlinee) = inlinees.get(&site.inlinee) {
                        self.handle_inlinee(site, parent_offset, inlinee, &program)?
                    } else {
                        None
                    }
                }
                // We need to ignore errors here since the PDB crate does not yet implement all
                // symbol types. Instead of erroring too often, it's better to swallow these.
                _ => continue,
            };

            match function {
                Some(function) => stack.push(depth, function),
                None => skipped_depth = Some(depth),
            }
        }

        // We're done, flush the remaining stack.
        stack.flush(0, &mut functions);

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

            return Some(Unit::load(debug_info, module));
        }

        None
    }
}

/// An iterator over source files in a Pdb object.
pub struct PdbFileIterator<'s> {
    debug_info: &'s PdbDebugInfo<'s>,
    units: PdbUnitIterator<'s>,
    files: pdb::FileIterator<'s>,
    finished: bool,
}

impl<'s> Iterator for PdbFileIterator<'s> {
    type Item = Result<FileEntry<'s>, PdbError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        loop {
            if let Some(file_result) = self.files.next().transpose() {
                let result = file_result
                    .map_err(|err| err.into())
                    .and_then(|i| self.debug_info.file_info(i))
                    .map(|info| FileEntry {
                        compilation_dir: &[],
                        info,
                    });

                return Some(result);
            }

            let unit = match self.units.next() {
                Some(Ok(unit)) => unit,
                Some(Err(error)) => return Some(Err(error)),
                None => break,
            };

            let line_program = match unit.module.line_program() {
                Ok(line_program) => line_program,
                Err(error) => return Some(Err(error.into())),
            };

            self.files = line_program.files();
        }

        self.finished = true;
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
