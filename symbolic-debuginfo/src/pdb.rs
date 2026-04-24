//! Support for Program Database, the debug companion format on Windows.

use std::borrow::Cow;
use std::collections::btree_map::BTreeMap;
use std::error::Error;
use std::fmt;
use std::io::Cursor;
use std::sync::Arc;

use elsa::FrozenMap;
use parking_lot::RwLock;
use pdb_addr2line::pdb::{
    AddressMap, FallibleIterator, ImageSectionHeader, InlineSiteSymbol, LineProgram, MachineType,
    Module, ModuleInfo, PdbInternalSectionOffset, ProcedureSymbol, RawString,
    RegisterRelativeSymbol, RegisterVariableSymbol, SeparatedCodeSymbol, SymbolData, TypeData,
    TypeFinder, TypeIndex,
};
use pdb_addr2line::ModuleProvider;
use smallvec::SmallVec;
use srcsrv;
use thiserror::Error;

use symbolic_common::{
    Arch, AsSelf, CodeId, CpuFamily, DebugId, Language, Name, NameMangling, SelfCell, Uuid,
};

use crate::base::*;
use crate::function_stack::FunctionStack;
use crate::sourcebundle::SourceFileDescriptor;

type Pdb<'data> = pdb::PDB<'data, Cursor<&'data [u8]>>;

const MAGIC_BIG: &[u8] = b"Microsoft C/C++ MSF 7.00\r\n\x1a\x44\x53\x00\x00\x00";

// Used for CFI, remove once abstraction is complete
#[doc(hidden)]
pub use pdb_addr2line::pdb;

/// The error type for [`PdbError`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdbErrorKind {
    /// The PDB file is corrupted. See the cause for more information.
    BadObject,

    /// An inline record was encountered without an inlining parent.
    UnexpectedInline,

    /// Formatting of a type name failed.
    FormattingFailed,
}

impl fmt::Display for PdbErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadObject => write!(f, "invalid pdb file"),
            Self::UnexpectedInline => write!(f, "unexpected inline function without parent"),
            Self::FormattingFailed => write!(f, "failed to format type name"),
        }
    }
}

/// An error when dealing with [`PdbObject`](struct.PdbObject.html).
#[derive(Debug, Error)]
#[error("{kind}")]
pub struct PdbError {
    kind: PdbErrorKind,
    #[source]
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl PdbError {
    /// Creates a new PDB error from a known kind of error as well as an arbitrary error
    /// payload.
    fn new<E>(kind: PdbErrorKind, source: E) -> Self
    where
        E: Into<Box<dyn Error + Send + Sync>>,
    {
        let source = Some(source.into());
        Self { kind, source }
    }

    /// Returns the corresponding [`PdbErrorKind`] for this error.
    pub fn kind(&self) -> PdbErrorKind {
        self.kind
    }
}

impl From<PdbErrorKind> for PdbError {
    fn from(kind: PdbErrorKind) -> Self {
        Self { kind, source: None }
    }
}

impl From<pdb::Error> for PdbError {
    fn from(e: pdb::Error) -> Self {
        Self::new(PdbErrorKind::BadObject, e)
    }
}

impl From<fmt::Error> for PdbError {
    fn from(e: fmt::Error) -> Self {
        Self::new(PdbErrorKind::FormattingFailed, e)
    }
}

impl From<pdb_addr2line::Error> for PdbError {
    fn from(e: pdb_addr2line::Error) -> Self {
        match e {
            pdb_addr2line::Error::PdbError(e) => Self::new(PdbErrorKind::BadObject, e),
            pdb_addr2line::Error::FormatError(e) => Self::new(PdbErrorKind::FormattingFailed, e),
            e => Self::new(PdbErrorKind::FormattingFailed, e),
        }
    }
}

/// Program Database, the debug companion format on Windows.
///
/// This object is a sole debug companion to [`PeObject`](../pdb/struct.PdbObject.html).
pub struct PdbObject<'data> {
    pdb: Arc<RwLock<Pdb<'data>>>,
    debug_info: Arc<pdb::DebugInformation<'data>>,
    pdb_info: pdb::PDBInformation<'data>,
    public_syms: pdb::SymbolTable<'data>,
    executable_sections: ExecutableSections,
    data: &'data [u8],
}

// NB: The pdb crate simulates mmap behavior on any Read + Seek type. This implementation requires
// mutability of the `Source` and uses trait objects without a Send + Sync barrier. We know that we
// only instanciate `&[u8]` as source. Whenever we mutate the reader (to read a new module stream),
// we acquire a write lock on the PDB, which should be sufficient.
unsafe impl Send for PdbObject<'_> {}
unsafe impl Sync for PdbObject<'_> {}

impl<'data> PdbObject<'data> {
    /// Tests whether the buffer could contain an PDB object.
    pub fn test(data: &[u8]) -> bool {
        // NB: "Microsoft C/C++ program database 2.00" is not supported by the pdb crate, so there
        // is no point in pretending we could read it.
        data.starts_with(MAGIC_BIG)
    }

    /// Tries to parse a PDB object from the given slice.
    #[allow(clippy::arc_with_non_send_sync)]
    pub fn parse(data: &'data [u8]) -> Result<Self, PdbError> {
        let mut pdb = Pdb::open(Cursor::new(data))?;
        let dbi = pdb.debug_information()?;
        let pdbi = pdb.pdb_information()?;
        let pubi = pdb.global_symbols()?;
        let sections = pdb.sections()?;

        Ok(PdbObject {
            pdb: Arc::new(RwLock::new(pdb)),
            debug_info: Arc::new(dbi),
            pdb_info: pdbi,
            public_syms: pubi,
            data,
            executable_sections: ExecutableSections::from_sections(&sections),
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
    pub fn symbols(&self) -> PdbSymbolIterator<'data, '_> {
        PdbSymbolIterator {
            symbols: self.public_syms.iter(),
            address_map: self.pdb.write().address_map().ok(),
            executable_sections: &self.executable_sections,
        }
    }

    /// Returns an ordered map of symbols in the symbol table.
    pub fn symbol_map(&self) -> SymbolMap<'data> {
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

    /// Returns the SRCSRV VCS integration name if available.
    ///
    /// This extracts the version control system identifier from the SRCSRV stream,
    /// if present. Common values include "perforce", "tfs", "git", etc.
    /// Returns `None` if no SRCSRV stream exists or if it cannot be parsed.
    pub fn srcsrv_vcs_name(&self) -> Option<String> {
        let mut pdb = self.pdb.write();

        // Try to open the "srcsrv" named stream
        let stream = match pdb.named_stream(b"srcsrv") {
            Ok(stream) => stream,
            Err(_) => return None,
        };

        // Parse the stream to extract VCS name
        let stream_data = stream.as_slice();
        if let Ok(parsed_stream) = srcsrv::SrcSrvStream::parse(stream_data) {
            parsed_stream
                .version_control_description()
                .map(|s| s.to_string())
        } else {
            None
        }
    }

    /// Determines whether this object is malformed and was only partially parsed
    pub fn is_malformed(&self) -> bool {
        false
    }

    /// Constructs a debugging session.
    pub fn debug_session(&self) -> Result<PdbDebugSession<'data>, PdbError> {
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
    pub fn data(&self) -> &'data [u8] {
        self.data
    }

    #[doc(hidden)]
    pub fn inner(&self) -> &RwLock<Pdb<'data>> {
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
            .field("is_malformed", &self.is_malformed())
            .finish()
    }
}

impl<'slf, 'data: 'slf> AsSelf<'slf> for PdbObject<'data> {
    type Ref = PdbObject<'slf>;

    fn as_self(&'slf self) -> &'slf Self::Ref {
        unsafe { std::mem::transmute(self) }
    }
}

impl<'data> Parse<'data> for PdbObject<'data> {
    type Error = PdbError;

    fn test(data: &[u8]) -> bool {
        Self::test(data)
    }

    fn parse(data: &'data [u8]) -> Result<Self, PdbError> {
        Self::parse(data)
    }
}

impl<'data: 'object, 'object> ObjectLike<'data, 'object> for PdbObject<'data> {
    type Error = PdbError;
    type Session = PdbDebugSession<'data>;
    type SymbolIterator = PdbSymbolIterator<'data, 'object>;

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

    fn symbols(&'object self) -> Self::SymbolIterator {
        self.symbols()
    }

    fn symbol_map(&self) -> SymbolMap<'data> {
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

    fn is_malformed(&self) -> bool {
        self.is_malformed()
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

/// Contains information about which sections are executable.
struct ExecutableSections {
    /// For every section header in the PDB, a boolean which indicates whether the "executable"
    /// or "execute" flag is set in the section header's characteristics.
    is_executable_per_section: Vec<bool>,
}

impl ExecutableSections {
    pub fn from_sections(sections: &Option<Vec<ImageSectionHeader>>) -> Self {
        Self {
            is_executable_per_section: match sections {
                Some(sections) => sections
                    .iter()
                    .map(|section| section.characteristics)
                    .map(|char| char.executable() || char.execute())
                    .collect(),
                None => Default::default(),
            },
        }
    }

    /// Returns whether the given offset is contained in an executable section.
    pub fn contains(&self, offset: &PdbInternalSectionOffset) -> bool {
        // offset.section is a one-based index.
        if offset.section == 0 {
            // No section.
            return false;
        }

        let section_index = (offset.section - 1) as usize;
        self.is_executable_per_section
            .get(section_index)
            .cloned()
            .unwrap_or(false)
    }
}

/// An iterator over symbols in the PDB file.
///
/// Returned by [`PdbObject::symbols`](struct.PdbObject.html#method.symbols).
pub struct PdbSymbolIterator<'data, 'object> {
    symbols: pdb::SymbolIter<'object>,
    address_map: Option<AddressMap<'data>>,
    executable_sections: &'object ExecutableSections,
}

impl<'data> Iterator for PdbSymbolIterator<'data, '_> {
    type Item = Symbol<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        let address_map = self.address_map.as_ref()?;

        while let Ok(Some(symbol)) = self.symbols.next() {
            if let Ok(SymbolData::Public(public)) = symbol.parse() {
                if !self.executable_sections.contains(&public.offset) {
                    continue;
                }

                let address = match public.offset.to_rva(address_map) {
                    Some(address) => address,
                    None => continue,
                };

                // pdb::SymbolIter offers data bound to its own lifetime since it holds the
                // buffer containing public symbols. The contract requires that we return
                // `Symbol<'data>`, so we cannot return zero-copy symbols here.
                let cow = public.name.to_string();
                let name = Cow::from(String::from(cow));

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

struct PdbStreams<'d> {
    debug_info: Arc<pdb::DebugInformation<'d>>,
    type_info: pdb::TypeInformation<'d>,
    id_info: pdb::IdInformation<'d>,
    string_table: Option<pdb::StringTable<'d>>,

    pdb: Arc<RwLock<Pdb<'d>>>,

    /// ModuleInfo objects are stored on this object (outside PdbDebugInfo) so that the
    /// PdbDebugInfo can store a TypeFormatter, which has a lifetime dependency on its
    /// ModuleProvider, which is this PdbStreams. This is so that TypeFormatter can cache
    /// CrossModuleImports inside itself, and those have a lifetime dependency on the
    /// ModuleInfo.
    module_infos: FrozenMap<usize, Box<ModuleInfo<'d>>>,
}

impl<'d> PdbStreams<'d> {
    fn from_pdb(pdb: &PdbObject<'d>) -> Result<Self, PdbError> {
        let mut p = pdb.pdb.write();

        // PDB::string_table errors if the named stream for the string table is not present.
        // However, this occurs in certain PDBs and does not automatically indicate an error.
        let string_table = match p.string_table() {
            Ok(string_table) => Some(string_table),
            Err(pdb::Error::StreamNameNotFound) => None,
            Err(e) => return Err(e.into()),
        };

        Ok(Self {
            string_table,
            debug_info: pdb.debug_info.clone(),
            type_info: p.type_information()?,
            id_info: p.id_information()?,
            pdb: pdb.pdb.clone(),
            module_infos: FrozenMap::new(),
        })
    }
}

impl<'d> pdb_addr2line::ModuleProvider<'d> for PdbStreams<'d> {
    fn get_module_info(
        &self,
        module_index: usize,
        module: &Module,
    ) -> Result<Option<&ModuleInfo<'d>>, pdb::Error> {
        if let Some(module_info) = self.module_infos.get(&module_index) {
            return Ok(Some(module_info));
        }

        let mut pdb = self.pdb.write();
        Ok(pdb.module_info(module)?.map(|module_info| {
            self.module_infos
                .insert(module_index, Box::new(module_info))
        }))
    }
}

struct PdbDebugInfo<'d> {
    /// The streams, to load module streams on demand.
    streams: &'d PdbStreams<'d>,
    /// OMAP structure to map reordered sections to RVAs.
    address_map: pdb::AddressMap<'d>,
    /// String table for name lookups.
    string_table: Option<&'d pdb::StringTable<'d>>,
    /// Type formatter for function name strings.
    type_formatter: pdb_addr2line::TypeFormatter<'d, 'd>,
    /// Type finder for resolving complex type indices to their definitions.
    type_finder: TypeFinder<'d>,
}

impl<'d> PdbDebugInfo<'d> {
    fn build(pdb: &PdbObject<'d>, streams: &'d PdbStreams<'d>) -> Result<Self, PdbError> {
        let modules = streams.debug_info.modules()?.collect::<Vec<_>>()?;

        // Avoid deadlocks by only covering the two access to the address map. For
        // instance, `pdb.symbol_map()` requires a mutable borrow of the PDB as well.
        let mut p = pdb.pdb.write();
        let address_map = p.address_map()?;

        drop(p);

        // Build the type finder by iterating through all type records.
        // This populates an index that allows O(1) lookup of any type by TypeIndex.
        let mut type_finder = streams.type_info.finder();
        let mut type_iter = streams.type_info.iter();
        while type_iter.next()?.is_some() {
            type_finder.update(&type_iter);
        }

        Ok(PdbDebugInfo {
            address_map,
            streams,
            string_table: streams.string_table.as_ref(),
            type_formatter: pdb_addr2line::TypeFormatter::new_from_parts(
                streams,
                modules,
                &streams.debug_info,
                &streams.type_info,
                &streams.id_info,
                streams.string_table.as_ref(),
                Default::default(),
            )?,
            type_finder,
        })
    }

    /// Returns an iterator over all compilation units (modules).
    fn units(&'d self) -> PdbUnitIterator<'d> {
        PdbUnitIterator {
            debug_info: self,
            index: 0,
        }
    }

    fn modules(&self) -> &[Module<'d>] {
        self.type_formatter.modules()
    }

    fn get_module(&'d self, index: usize) -> Result<Option<&'d ModuleInfo<'d>>, PdbError> {
        // Silently ignore module references out-of-bound
        let module = match self.modules().get(index) {
            Some(module) => module,
            None => return Ok(None),
        };

        Ok(self.streams.get_module_info(index, module)?)
    }

    fn file_info(&self, file_info: pdb::FileInfo<'d>) -> Result<FileInfo<'_>, PdbError> {
        let file_path = match self.string_table {
            Some(string_table) => file_info.name.to_raw_string(string_table)?,
            None => "".into(),
        };

        Ok(FileInfo::from_path(file_path.as_bytes()))
    }
}

impl<'slf, 'd: 'slf> AsSelf<'slf> for PdbDebugInfo<'d> {
    type Ref = PdbDebugInfo<'slf>;

    fn as_self(&'slf self) -> &'slf Self::Ref {
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

    /// See [DebugSession::source_by_path] for more information.
    pub fn source_by_path(
        &self,
        _path: &str,
    ) -> Result<Option<SourceFileDescriptor<'_>>, PdbError> {
        Ok(None)
    }
}

impl<'session> DebugSession<'session> for PdbDebugSession<'_> {
    type Error = PdbError;
    type FunctionIterator = PdbFunctionIterator<'session>;
    type FileIterator = PdbFileIterator<'session>;

    fn functions(&'session self) -> Self::FunctionIterator {
        self.functions()
    }

    fn files(&'session self) -> Self::FileIterator {
        self.files()
    }

    fn source_by_path(&self, path: &str) -> Result<Option<SourceFileDescriptor<'_>>, Self::Error> {
        self.source_by_path(path)
    }
}

struct Unit<'s> {
    debug_info: &'s PdbDebugInfo<'s>,
    module_index: usize,
    module: &'s pdb::ModuleInfo<'s>,
}

impl<'s> Unit<'s> {
    fn load(
        debug_info: &'s PdbDebugInfo<'s>,
        module_index: usize,
        module: &'s pdb::ModuleInfo<'s>,
    ) -> Result<Self, PdbError> {
        Ok(Self {
            debug_info,
            module_index,
            module,
        })
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
            let rva = match line_info.offset.to_rva(address_map) {
                Some(rva) => u64::from(rva.0),
                None => continue,
            };

            // skip 0-sized line infos
            let size = line_info.length.map(u64::from);
            if size == Some(0) {
                continue;
            }

            let file_info = program.get_file_info(line_info.file_index)?;

            lines.push(LineInfo {
                address: rva,
                size,
                file: self.debug_info.file_info(file_info)?,
                line: line_info.line_start.into(),
            });
        }
        lines.sort_by_key(|line| line.address);

        // Merge line infos that only differ in their `column` information, which we don't
        // care about. We only want to output line infos that differ in their file/line.
        lines.dedup_by(|current, prev| {
            // the records need to be consecutive to be able to merge
            let first_end = prev.size.and_then(|size| prev.address.checked_add(size));
            let is_consecutive = first_end == Some(current.address);
            // the line record points to the same file/line, so we want to merge/dedupe it
            if is_consecutive && prev.file == current.file && prev.line == current.line {
                prev.size = prev
                    .size
                    .map(|first_size| first_size.saturating_add(current.size.unwrap_or(0)));

                return true;
            }
            false
        });

        Ok(lines)
    }

    /// Sanitize the collected lines.
    ///
    /// This essentially filters out all the lines that lay outside of the function range.
    ///
    /// For example we have observed in a real-world pdb that has:
    /// - A function 0x33ea50 (size 0xc)
    /// - With one line record: 0x33e850 (size 0x26)
    ///
    /// The line record is completely outside the range of the function.
    fn sanitize_lines(func: &mut Function) {
        let fn_start = func.address;
        let fn_end = func.end_address();
        func.lines.retain(|line| {
            if line.address >= fn_end {
                return false;
            }
            let line_end = match line.size {
                Some(size) => line.address.saturating_add(size),
                None => return true,
            };
            line_end > fn_start
        });
    }

    fn handle_function(
        &self,
        offset: PdbInternalSectionOffset,
        len: u32,
        name: RawString<'s>,
        type_index: TypeIndex,
        program: &LineProgram<'s>,
    ) -> Result<Option<Function<'s>>, PdbError> {
        let address_map = &self.debug_info.address_map;

        // Translate the function's address to the PE's address space. If this fails, we're
        // likely dealing with an invalid function and can skip it.
        let address = match offset.to_rva(address_map) {
            Some(addr) => u64::from(addr.0),
            None => return Ok(None),
        };

        // Names from the private symbol table are generally demangled. They contain the path of the
        // scope and name of the function itself, including type parameters, and the parameter lists
        // are contained in the type info. We do not emit a return type.
        let formatter = &self.debug_info.type_formatter;
        let name = name.to_string();
        let name = Name::new(
            formatter
                .format_function(&name, self.module_index, type_index)
                .map(Cow::Owned)
                .unwrap_or(name),
            NameMangling::Unmangled,
            Language::Unknown,
        );

        let line_iter = program.lines_for_symbol(offset);
        let lines = self.collect_lines(line_iter, program)?;

        Ok(Some(Function {
            address,
            size: len.into(),
            name,
            compilation_dir: &[],
            lines,
            inlinees: Vec::new(),
            inline: false,
            variables: Vec::new(),
        }))
    }

    fn handle_procedure(
        &self,
        proc: &ProcedureSymbol<'s>,
        program: &LineProgram<'s>,
    ) -> Result<Option<Function<'s>>, PdbError> {
        self.handle_function(proc.offset, proc.len, proc.name, proc.type_index, program)
    }

    fn handle_separated_code(
        &self,
        proc: &ProcedureSymbol<'s>,
        sepcode: &SeparatedCodeSymbol,
        program: &LineProgram<'s>,
    ) -> Result<Option<Function<'s>>, PdbError> {
        self.handle_function(
            sepcode.offset,
            sepcode.len,
            proc.name,
            proc.type_index,
            program,
        )
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
        let start = match lines.first().map(|line| line.address) {
            Some(address) => address,
            None => return Ok(None),
        };

        let end = match lines
            .last()
            .map(|line| line.address + line.size.unwrap_or(1))
        {
            Some(address) => address,
            None => return Ok(None),
        };

        let formatter = &self.debug_info.type_formatter;
        let name = Name::new(
            formatter.format_id(self.module_index, inline_site.inlinee)?,
            NameMangling::Unmangled,
            Language::Unknown,
        );

        Ok(Some(Function {
            address: start,
            size: end - start,
            name,
            compilation_dir: &[],
            lines,
            inlinees: Vec::new(),
            inline: true,
            variables: Vec::new(),
        }))
    }

    /// Creates a [`Variable`] from an `S_REGREL32` symbol (register-relative, e.g. stack variable).
    fn make_regrel_variable(&self, regrel: &RegisterRelativeSymbol<'s>) -> Option<Variable<'s>> {
        let name = regrel.name.to_string();
        let name_str = name.to_string();
        if name_str.is_empty() {
            return None;
        }
        let (type_name, type_info) = self.resolve_pdb_type(regrel.type_index);
        Some(Variable {
            name: Cow::Owned(name_str),
            type_name,
            type_info,
            is_parameter: false,
            location: VariableLocation::RegisterRelative {
                register: regrel.register.0,
                offset: regrel.offset as i64,
            },
            scope: None,
        })
    }

    /// Creates a [`Variable`] from an `S_REGISTER` symbol (variable in a CPU register).
    fn make_register_variable(
        &self,
        regvar: &RegisterVariableSymbol<'s>,
    ) -> Option<Variable<'s>> {
        let name = regvar.name.to_string();
        let name_str = name.to_string();
        if name_str.is_empty() {
            return None;
        }
        let (type_name, type_info) = self.resolve_pdb_type(regvar.type_index);
        Some(Variable {
            name: Cow::Owned(name_str),
            type_name,
            type_info,
            is_parameter: false,
            location: VariableLocation::Register(regvar.register.0),
            scope: None,
        })
    }

    /// Creates a [`Variable`] from an `S_LOCAL` symbol (modern local variable record).
    ///
    /// `S_LOCAL` symbols are followed by `S_DEFRANGE_*` records that describe where the
    /// variable lives. Since the `pdb` crate (v0.8) does not expose DefRange records as
    /// parsed `SymbolData` variants, we mark the location as [`VariableLocation::OptimizedOut`]
    /// when `flags.isoptimizedout` is set, and as `OptimizedOut` otherwise (to be improved
    /// when DefRange parsing is available).
    fn make_local_variable(&self, local: &pdb::LocalSymbol<'s>) -> Variable<'s> {
        let name = local.name.to_string();
        let (type_name, type_info) = self.resolve_pdb_type(local.type_index);
        let location = if local.flags.isoptimizedout {
            VariableLocation::OptimizedOut
        } else {
            // Without DefRange parsing, we cannot determine the exact location.
            // Mark as OptimizedOut for now; Phase 2 can improve this.
            VariableLocation::OptimizedOut
        };
        Variable {
            name: Cow::Owned(name.to_string()),
            type_name,
            type_info,
            is_parameter: local.flags.isparam,
            location,
            scope: None,
        }
    }

    /// Resolves a PDB [`TypeIndex`] to a display name and [`VariableType`].
    ///
    /// Handles PDB primitive type indices (< 0x1000) directly by decoding the kind
    /// and indirection bits. Complex types are resolved via the [`TypeFinder`] to
    /// extract struct fields, array dimensions, enum variants, etc.
    fn resolve_pdb_type(&self, type_index: TypeIndex) -> (Cow<'s, str>, VariableType) {
        self.resolve_pdb_type_depth(type_index, 0)
    }

    /// Inner recursive type resolver with depth tracking to prevent infinite recursion.
    fn resolve_pdb_type_depth(
        &self,
        type_index: TypeIndex,
        depth: usize,
    ) -> (Cow<'s, str>, VariableType) {
        const MAX_DEPTH: usize = 5;
        const MAX_STRUCT_FIELDS: usize = 32;

        let raw = type_index.0;
        if raw < 0x1000 {
            return Self::resolve_pdb_primitive(raw);
        }

        if depth > MAX_DEPTH {
            return (Cow::Borrowed("..."), VariableType::Unknown { byte_size: 0 });
        }

        // Try to find and parse the type record
        let type_data = match self.debug_info.type_finder.find(type_index) {
            Ok(item) => match item.parse() {
                Ok(data) => data,
                Err(_) => return self.resolve_pdb_type_fallback(type_index),
            },
            Err(_) => return self.resolve_pdb_type_fallback(type_index),
        };

        match type_data {
            // Struct, class, or interface
            TypeData::Class(class) => {
                let name = class.name.to_string().to_string();

                // Forward references have no fields; resolve via unique_name if possible
                if class.properties.forward_reference() {
                    return (
                        Cow::Owned(name.clone()),
                        VariableType::Struct {
                            name,
                            byte_size: class.size as u32,
                            fields: Vec::new(),
                        },
                    );
                }

                let mut fields = Vec::new();
                if let Some(field_list_idx) = class.fields {
                    self.collect_pdb_fields(field_list_idx, &mut fields, depth, MAX_STRUCT_FIELDS);
                }

                (
                    Cow::Owned(name.clone()),
                    VariableType::Struct {
                        name,
                        byte_size: class.size as u32,
                        fields,
                    },
                )
            }

            // Union type
            TypeData::Union(union) => {
                let name = union.name.to_string().to_string();

                if union.properties.forward_reference() {
                    return (
                        Cow::Owned(name.clone()),
                        VariableType::Struct {
                            name,
                            byte_size: union.size as u32,
                            fields: Vec::new(),
                        },
                    );
                }

                let mut fields = Vec::new();
                self.collect_pdb_fields(union.fields, &mut fields, depth, MAX_STRUCT_FIELDS);

                (
                    Cow::Owned(name.clone()),
                    VariableType::Struct {
                        name,
                        byte_size: union.size as u32,
                        fields,
                    },
                )
            }

            // Array type
            TypeData::Array(array) => {
                let (element_name, element_type) =
                    self.resolve_pdb_type_depth(array.element_type, depth + 1);
                let elem_size = element_type.byte_size().unwrap_or(1).max(1);
                // Dimensions: first dimension is total byte size, compute element count
                let total_size = array.dimensions.first().copied().unwrap_or(0) as u64;
                let count = total_size / elem_size;
                let display_name = format!("{}[{}]", element_name, count);

                (
                    Cow::Owned(display_name),
                    VariableType::Array {
                        element_type_name: element_name.into_owned(),
                        count,
                        byte_size: total_size as u32,
                    },
                )
            }

            // Enumeration type
            TypeData::Enumeration(enumeration) => {
                let name = enumeration.name.to_string().to_string();
                let (_, underlying) =
                    self.resolve_pdb_type_depth(enumeration.underlying_type, depth + 1);
                let byte_size = underlying.byte_size().unwrap_or(4) as u16;

                // Collect enum variants from the field list
                let mut variants = Vec::new();
                self.collect_pdb_enum_variants(enumeration.fields, &mut variants);

                (
                    Cow::Owned(name.clone()),
                    VariableType::Enum {
                        name,
                        byte_size,
                        variants,
                    },
                )
            }

            // Pointer type
            TypeData::Pointer(pointer) => {
                let (pointee_name, _) =
                    self.resolve_pdb_type_depth(pointer.underlying_type, depth + 1);
                let ptr_size = pointer.attributes.size() as u16;

                let display_name = if pointer.containing_class.is_some() {
                    // Pointer-to-member
                    let class_name = pointer
                        .containing_class
                        .map(|ci| self.resolve_pdb_type_depth(ci, depth + 1).0.into_owned())
                        .unwrap_or_else(|| "?".to_string());
                    format!("{} {}::*", pointee_name, class_name)
                } else if pointer.attributes.pointer_mode() == pdb::PointerMode::LValueReference {
                    format!("{}&", pointee_name)
                } else if pointer.attributes.pointer_mode() == pdb::PointerMode::RValueReference {
                    format!("{}&&", pointee_name)
                } else {
                    format!("{}*", pointee_name)
                };

                (
                    Cow::Owned(display_name),
                    VariableType::Pointer {
                        pointee_type_name: pointee_name.into_owned(),
                        byte_size: ptr_size,
                    },
                )
            }

            // Modifier (const, volatile, unaligned)
            TypeData::Modifier(modifier) => {
                self.resolve_pdb_type_depth(modifier.underlying_type, depth + 1)
            }

            // Procedure (function pointer)
            TypeData::Procedure(proc) => {
                let ret_name = proc
                    .return_type
                    .map(|ti| self.resolve_pdb_type_depth(ti, depth + 1).0.into_owned())
                    .unwrap_or_else(|| "void".to_string());

                let params = self.collect_pdb_argument_names(proc.argument_list, depth);
                let display_name = format!("{}(*)({})", ret_name, params.join(", "));
                let ptr_size = self
                    .debug_info
                    .type_formatter
                    .get_type_size(self.module_index, type_index) as u16;

                (
                    Cow::Owned(display_name),
                    VariableType::Pointer {
                        pointee_type_name: format!("{}({})", ret_name, params.join(", ")),
                        byte_size: if ptr_size > 0 { ptr_size } else { 8 },
                    },
                )
            }

            // Member function
            TypeData::MemberFunction(mfn) => {
                let ret_name = self
                    .resolve_pdb_type_depth(mfn.return_type, depth + 1)
                    .0
                    .into_owned();
                let class_name = self
                    .resolve_pdb_type_depth(mfn.class_type, depth + 1)
                    .0
                    .into_owned();
                let params = self.collect_pdb_argument_names(mfn.argument_list, depth);
                let display_name =
                    format!("{} ({}::*)({})", ret_name, class_name, params.join(", "));

                let ptr_size = self
                    .debug_info
                    .type_formatter
                    .get_type_size(self.module_index, type_index) as u16;

                (
                    Cow::Owned(display_name),
                    VariableType::Pointer {
                        pointee_type_name: format!(
                            "{} {}::*({})",
                            ret_name,
                            class_name,
                            params.join(", ")
                        ),
                        byte_size: if ptr_size > 0 { ptr_size } else { 8 },
                    },
                )
            }

            // Bitfield — resolve the underlying type
            TypeData::Bitfield(bitfield) => {
                self.resolve_pdb_type_depth(bitfield.underlying_type, depth + 1)
            }

            // Anything else — fall back to formatter
            _ => self.resolve_pdb_type_fallback(type_index),
        }
    }

    /// Fallback for types we can't fully resolve: use the type formatter for size.
    fn resolve_pdb_type_fallback(&self, type_index: TypeIndex) -> (Cow<'s, str>, VariableType) {
        let byte_size = self
            .debug_info
            .type_formatter
            .get_type_size(self.module_index, type_index);

        // Try to get at least the name from TypeData
        let name = self
            .debug_info
            .type_finder
            .find(type_index)
            .ok()
            .and_then(|item| item.parse().ok())
            .and_then(|data: TypeData<'_>| data.name().map(|n| n.to_string().to_string()));

        (
            name.map(Cow::Owned).unwrap_or(Cow::Borrowed("?")),
            VariableType::Unknown {
                byte_size: byte_size as u16,
            },
        )
    }

    /// Collects struct/class fields from a PDB FieldList type record.
    fn collect_pdb_fields(
        &self,
        field_list_idx: TypeIndex,
        fields: &mut Vec<StructField>,
        depth: usize,
        max_fields: usize,
    ) {
        let field_list = match self.debug_info.type_finder.find(field_list_idx) {
            Ok(item) => match item.parse() {
                Ok(TypeData::FieldList(fl)) => fl,
                _ => return,
            },
            Err(_) => return,
        };

        for field_data in &field_list.fields {
            if fields.len() >= max_fields {
                break;
            }
            match field_data {
                TypeData::Member(member) => {
                    let field_name = member.name.to_string().to_string();
                    let (field_type_name, field_type_info) =
                        self.resolve_pdb_type_depth(member.field_type, depth + 1);
                    let field_byte_size = field_type_info.byte_size().unwrap_or(0);
                    fields.push(StructField {
                        name: field_name,
                        type_name: field_type_name.into_owned(),
                        type_info: field_type_info,
                        offset: member.offset as u64,
                        byte_size: field_byte_size,
                    });
                }
                TypeData::BaseClass(base) => {
                    let (base_type_name, base_type_info) =
                        self.resolve_pdb_type_depth(base.base_class, depth + 1);
                    let base_byte_size = base_type_info.byte_size().unwrap_or(0);
                    fields.push(StructField {
                        name: format!("__base_{}", base_type_name),
                        type_name: base_type_name.into_owned(),
                        type_info: base_type_info,
                        offset: base.offset as u64,
                        byte_size: base_byte_size,
                    });
                }
                TypeData::StaticMember(_) | TypeData::Method(_) | TypeData::OverloadedMethod(_) => {
                    // Skip static members and methods — they're not instance fields
                }
                _ => {}
            }
        }

        // Handle continuation records
        if let Some(continuation) = field_list.continuation {
            if fields.len() < max_fields {
                self.collect_pdb_fields(continuation, fields, depth, max_fields);
            }
        }
    }

    /// Collects enum variant names and values from a PDB FieldList.
    fn collect_pdb_enum_variants(
        &self,
        field_list_idx: TypeIndex,
        variants: &mut Vec<(String, i64)>,
    ) {
        let field_list = match self.debug_info.type_finder.find(field_list_idx) {
            Ok(item) => match item.parse() {
                Ok(TypeData::FieldList(fl)) => fl,
                _ => return,
            },
            Err(_) => return,
        };

        for field_data in &field_list.fields {
            if let TypeData::Enumerate(enumerate) = field_data {
                let name = enumerate.name.to_string().to_string();
                let value = match enumerate.value {
                    pdb::Variant::U8(v) => v as i64,
                    pdb::Variant::U16(v) => v as i64,
                    pdb::Variant::U32(v) => v as i64,
                    pdb::Variant::U64(v) => v as i64,
                    pdb::Variant::I8(v) => v as i64,
                    pdb::Variant::I16(v) => v as i64,
                    pdb::Variant::I32(v) => v as i64,
                    pdb::Variant::I64(v) => v,
                };
                variants.push((name, value));
            }
        }

        // Handle continuation records
        if let Some(continuation) = field_list.continuation {
            self.collect_pdb_enum_variants(continuation, variants);
        }
    }

    /// Collects argument type names from a PDB ArgumentList record.
    fn collect_pdb_argument_names(&self, arg_list_idx: TypeIndex, depth: usize) -> Vec<String> {
        let arg_list = match self.debug_info.type_finder.find(arg_list_idx) {
            Ok(item) => match item.parse() {
                Ok(TypeData::ArgumentList(al)) => al,
                _ => return Vec::new(),
            },
            Err(_) => return Vec::new(),
        };

        arg_list
            .arguments
            .iter()
            .map(|&ti| self.resolve_pdb_type_depth(ti, depth + 1).0.into_owned())
            .collect()
    }

    /// Decodes a PDB primitive type index (< 0x1000) into a name and [`VariableType`].
    ///
    /// PDB primitive type indices encode the base type in bits [0:7] and the
    /// pointer indirection mode in bits [8:11].
    fn resolve_pdb_primitive(raw: u32) -> (Cow<'s, str>, VariableType) {
        let kind = raw & 0xFF;
        let indirection = (raw >> 8) & 0xF;

        // If there's indirection, this is a pointer to a primitive type.
        if indirection != 0 {
            let ptr_size: u16 = match indirection {
                1 => 2,  // Near16
                2 | 3 => 4, // Far16, Huge16
                4 | 5 => 4, // Near32, Far32
                6 => 8,  // Near64
                _ => 8,
            };
            let (pointee_name, _) = Self::resolve_pdb_primitive(kind);
            return (
                Cow::Owned(format!("{}*", pointee_name)),
                VariableType::Pointer {
                    pointee_type_name: pointee_name.into_owned(),
                    byte_size: ptr_size,
                },
            );
        }

        match kind {
            // Void / NoType
            0x00 | 0x03 => (
                Cow::Borrowed("void"),
                VariableType::Unknown { byte_size: 0 },
            ),
            // Signed integers
            0x10 => (Cow::Borrowed("char"), VariableType::Primitive {
                encoding: PrimitiveEncoding::Char, byte_size: 1,
            }),
            0x68 => (Cow::Borrowed("int8_t"), VariableType::Primitive {
                encoding: PrimitiveEncoding::SignedInt, byte_size: 1,
            }),
            0x11 | 0x72 => (Cow::Borrowed("short"), VariableType::Primitive {
                encoding: PrimitiveEncoding::SignedInt, byte_size: 2,
            }),
            0x12 => (Cow::Borrowed("long"), VariableType::Primitive {
                encoding: PrimitiveEncoding::SignedInt, byte_size: 4,
            }),
            0x74 => (Cow::Borrowed("int"), VariableType::Primitive {
                encoding: PrimitiveEncoding::SignedInt, byte_size: 4,
            }),
            0x13 | 0x76 => (Cow::Borrowed("int64_t"), VariableType::Primitive {
                encoding: PrimitiveEncoding::SignedInt, byte_size: 8,
            }),
            // Unsigned integers
            0x20 => (Cow::Borrowed("unsigned char"), VariableType::Primitive {
                encoding: PrimitiveEncoding::UnsignedChar, byte_size: 1,
            }),
            0x69 => (Cow::Borrowed("uint8_t"), VariableType::Primitive {
                encoding: PrimitiveEncoding::UnsignedInt, byte_size: 1,
            }),
            0x21 | 0x73 => (Cow::Borrowed("unsigned short"), VariableType::Primitive {
                encoding: PrimitiveEncoding::UnsignedInt, byte_size: 2,
            }),
            0x22 => (Cow::Borrowed("unsigned long"), VariableType::Primitive {
                encoding: PrimitiveEncoding::UnsignedInt, byte_size: 4,
            }),
            0x75 => (Cow::Borrowed("unsigned int"), VariableType::Primitive {
                encoding: PrimitiveEncoding::UnsignedInt, byte_size: 4,
            }),
            0x23 | 0x77 => (Cow::Borrowed("uint64_t"), VariableType::Primitive {
                encoding: PrimitiveEncoding::UnsignedInt, byte_size: 8,
            }),
            // Wide char
            0x71 => (Cow::Borrowed("wchar_t"), VariableType::Primitive {
                encoding: PrimitiveEncoding::Char, byte_size: 2,
            }),
            0x7a => (Cow::Borrowed("char16_t"), VariableType::Primitive {
                encoding: PrimitiveEncoding::Char, byte_size: 2,
            }),
            0x7b => (Cow::Borrowed("char32_t"), VariableType::Primitive {
                encoding: PrimitiveEncoding::Char, byte_size: 4,
            }),
            // Floats
            0x40 => (Cow::Borrowed("float"), VariableType::Primitive {
                encoding: PrimitiveEncoding::Float, byte_size: 4,
            }),
            0x41 => (Cow::Borrowed("double"), VariableType::Primitive {
                encoding: PrimitiveEncoding::Float, byte_size: 8,
            }),
            0x42 => (Cow::Borrowed("long double"), VariableType::Primitive {
                encoding: PrimitiveEncoding::Float, byte_size: 10,
            }),
            // Booleans
            0x30 => (Cow::Borrowed("bool"), VariableType::Primitive {
                encoding: PrimitiveEncoding::Boolean, byte_size: 1,
            }),
            // HRESULT
            0x08 => (Cow::Borrowed("HRESULT"), VariableType::Primitive {
                encoding: PrimitiveEncoding::SignedInt, byte_size: 4,
            }),
            _ => (
                Cow::Borrowed("?"),
                VariableType::Unknown { byte_size: 0 },
            ),
        }
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
        let mut last_proc = None;

        while let Some(symbol) = symbols.next()? {
            if inc_next {
                depth += 1;
            }

            inc_next = symbol.starts_scope();
            if symbol.ends_scope() {
                depth -= 1;

                if proc_offsets.last().is_some_and(|&(d, _)| d >= depth) {
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
                    let function = self.handle_procedure(&proc, &program)?;
                    last_proc = Some(proc);
                    function
                }
                Ok(SymbolData::SeparatedCode(sepcode)) => match last_proc.as_ref() {
                    Some(last_proc) if last_proc.offset == sepcode.parent_offset => {
                        self.handle_separated_code(last_proc, &sepcode, &program)?
                    }
                    _ => continue,
                },
                Ok(SymbolData::InlineSite(site)) => {
                    let parent_offset = proc_offsets
                        .last()
                        .map(|&(_, offset)| offset)
                        .ok_or(PdbErrorKind::UnexpectedInline)?;

                    // We can assume that inlinees will be listed in the inlinee table. If missing,
                    // skip silently instead of erroring out. Missing a single inline function is
                    // more acceptable in such a case than halting iteration completely.
                    if let Some(inlinee) = inlinees.get(&site.inlinee) {
                        // We have seen that the MSVC Compiler `19.16` (VS 2017) can output
                        // `ChangeFile` annotations which are not properly aligned to the beginning
                        // of a file checksum, leading to `UnimplementedFileChecksumKind` errors.
                        // Investigation showed that this can happen for inlined `{ctor}` functions,
                        // but there are no clear leads to why that might have happened, and how to
                        // recover from these broken annotations.
                        // For that reason, we skip these inlinees completely so we do not fail
                        // processing the complete pdb file.
                        self.handle_inlinee(site, parent_offset, inlinee, &program)
                            .ok()
                            .flatten()
                    } else {
                        None
                    }
                }
                // Local variable symbols: extract and attach to the current function.
                Ok(SymbolData::RegisterRelative(ref regrel)) => {
                    if let Some(var) = self.make_regrel_variable(regrel) {
                        stack.add_variable_to_top(var);
                    }
                    continue;
                }
                Ok(SymbolData::RegisterVariable(ref regvar)) => {
                    if let Some(var) = self.make_register_variable(regvar) {
                        stack.add_variable_to_top(var);
                    }
                    continue;
                }
                Ok(SymbolData::Local(ref local)) => {
                    let var = self.make_local_variable(local);
                    stack.add_variable_to_top(var);
                    continue;
                }
                // We need to ignore errors here since the PDB crate does not yet implement all
                // symbol types. Instead of erroring too often, it's better to swallow these.
                _ => continue,
            };

            match function {
                Some(mut function) => {
                    Self::sanitize_lines(&mut function);
                    // TODO: figure out what to do with functions that have no more lines
                    // after sanitization
                    stack.push(depth, function)
                }
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
        while self.index < debug_info.modules().len() {
            let module_index = self.index;
            let result = debug_info.get_module(module_index);
            self.index += 1;

            let module = match result {
                Ok(Some(module)) => module,
                Ok(None) => continue,
                Err(error) => return Some(Err(error)),
            };

            return Some(Unit::load(debug_info, module_index, module));
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
                    .map(|info| FileEntry::new(Cow::default(), info));

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
