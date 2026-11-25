//! Support for Program Database, the debug companion format on Windows.

use std::borrow::Cow;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::sync::Arc;

use elsa::sync::FrozenMap;
use ms_pdb::codeview::parser::{Parser, ParserError};
use ms_pdb::coff::IMAGE_FILE_MACHINE;
use ms_pdb::dbi::ModuleInfoFixed;
use ms_pdb::lines::FileChecksumIter;
use ms_pdb::modi::ModiStreamData;
use ms_pdb::names::NameIndex;
use ms_pdb::syms::{Block, OffsetSegment, Proc, Sym, SymData, SymIter, SymKind};
use ms_pdb::types::TypeIndex;
use ms_pdb::{BStr, StreamData};
use smallvec::SmallVec;
use thiserror::Error;

use symbolic_common::{Arch, AsSelf, CodeId, CpuFamily, DebugId, Language, Name, NameMangling};

use crate::base::*;
use crate::pdb::srcsrv::{SourceServerInfo, SourceServerMappings};
use crate::sourcebundle::SourceFileDescriptor;
use crate::{ParseObjectOptions, Type, TypeRef};

use self::addresses::AddressMap;
use self::function_stack::{FunctionStack, ProcedureFrames};
use self::lines::{ModuleLines, RawLine};

mod addresses;
mod function_stack;
mod lines;
mod register;
mod srcsrv;
mod types;
mod variables;

type Pdb<'data> = ms_pdb::Pdb<&'data [u8]>;

/// A reference to a type in the PDB file.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PdbTypeRef(TypeIndex);

const MAGIC_BIG: &[u8] = b"Microsoft C/C++ MSF 7.00\r\n\x1a\x44\x53\x00\x00\x00";
const CV_SIGNATURE_C13: u32 = 4;
const S_THUNK32_ST: SymKind = SymKind(0x0206);
const S_BLOCK32_ST: SymKind = SymKind(0x0207);
const S_WITH32_ST: SymKind = SymKind(0x0208);

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

/// An error when dealing with [`PdbObject`].
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

impl From<anyhow::Error> for PdbError {
    fn from(e: anyhow::Error) -> Self {
        Self::new(PdbErrorKind::BadObject, e)
    }
}

impl From<ParserError> for PdbError {
    fn from(e: ParserError) -> Self {
        Self::new(PdbErrorKind::BadObject, e)
    }
}

impl From<fmt::Error> for PdbError {
    fn from(e: fmt::Error) -> Self {
        Self::new(PdbErrorKind::FormattingFailed, e)
    }
}

/// Program Database, the debug companion format on Windows.
///
/// This object is a sole debug companion to [`PeObject`](crate::pe::PeObject).
pub struct PdbObject<'data> {
    pdb: Arc<Pdb<'data>>,
    addresses: Arc<AddressMap>,
    data: &'data [u8],
}

impl<'data> PdbObject<'data> {
    /// Tests whether the buffer could contain an PDB object.
    pub fn test(data: &[u8]) -> bool {
        data.starts_with(MAGIC_BIG)
    }

    /// Tries to parse a PDB object from the given slice.
    pub fn parse(data: &'data [u8]) -> Result<Self, PdbError> {
        let pdb = Pdb::open_from_random_file(data)?;
        pdb.gss()?;
        let addresses = Arc::new(AddressMap::new(&pdb)?);

        Ok(PdbObject {
            pdb: Arc::from(pdb),
            addresses,
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
        let info = self.pdb.pdbi();
        let age = match self.pdb.dbi_header().age.get() {
            0 => info.age,
            age => age,
        };
        match info.unique_id {
            Some(uuid) => DebugId::from_parts(uuid, age),
            None => DebugId::default(),
        }
    }

    /// The CPU architecture of this object, as specified in the debug information stream (DBI).
    pub fn arch(&self) -> Arch {
        arch_from_machine(self.pdb.machine())
    }

    /// Returns true if this object contains source server information.
    pub fn has_source_server_data(&self) -> Result<bool, PdbError> {
        if let Some(index) = self.pdb.named_stream("srcsrv") {
            self.pdb.read_stream(index)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// The kind of this object, which is always `Debug`.
    pub fn kind(&self) -> ObjectKind {
        ObjectKind::Debug
    }

    /// The address at which the image prefers to be loaded into memory.
    ///
    /// The PDB only stores relative addresses, and more importantly, does not provide sufficient
    /// information to compute the original PE's load address. The according PE, however does
    /// feature a load address (called `image_base`).
    /// See [`PeObject::load_address`](crate::pe::PeObject::load_address) for more information.
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
            symbols: self
                .pdb
                .gss()
                .map_or_else(|_| SymIter::new(&[]), |gss| gss.iter_syms()),
            addresses: &self.addresses,
            marker: std::marker::PhantomData,
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

    /// Returns the raw data of the PDB file.
    pub fn data(&self) -> &'data [u8] {
        self.data
    }

    #[doc(hidden)]
    pub fn inner(&self) -> &ms_pdb::Pdb<&'data [u8]> {
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
        self
    }
}

impl<'data> Parse<'data> for PdbObject<'data> {
    type Error = PdbError;

    fn test(data: &[u8]) -> bool {
        Self::test(data)
    }

    fn parse_with_opts(data: &'data [u8], _opts: ParseObjectOptions) -> Result<Self, Self::Error> {
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

pub(crate) fn arch_from_machine(machine: IMAGE_FILE_MACHINE) -> Arch {
    match machine {
        IMAGE_FILE_MACHINE::IMAGE_FILE_MACHINE_I386 => Arch::X86,
        IMAGE_FILE_MACHINE::IMAGE_FILE_MACHINE_AMD64 => Arch::Amd64,
        IMAGE_FILE_MACHINE::IMAGE_FILE_MACHINE_ARM => Arch::Arm,
        IMAGE_FILE_MACHINE::IMAGE_FILE_MACHINE_ARM64 => Arch::Arm64,
        IMAGE_FILE_MACHINE::IMAGE_FILE_MACHINE_POWERPC => Arch::Ppc,
        _ => Arch::Unknown,
    }
}

/// An iterator over symbols in the PDB file.
///
/// Returned by [`PdbObject::symbols`].
pub struct PdbSymbolIterator<'data, 'object> {
    symbols: SymIter<'object>,
    addresses: &'object AddressMap,
    marker: std::marker::PhantomData<&'data [u8]>,
}

impl<'data> Iterator for PdbSymbolIterator<'data, '_> {
    type Item = Symbol<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        for symbol in self
            .symbols
            .by_ref()
            .filter(|symbol| matches!(symbol.kind, SymKind::S_PUB32 | SymKind::S_PUB32_ST))
        {
            let Ok(SymData::Pub(public)) = symbol.parse() else {
                continue;
            };
            let offset = public.offset_segment();
            if !self.addresses.is_executable(offset) {
                continue;
            }
            let Some(address) = self.addresses.rva(offset) else {
                continue;
            };
            return Some(Symbol {
                name: Some(String::from_utf8_lossy(public.name).into_owned().into()),
                address: address.into(),
                size: 0,
            });
        }
        None
    }
}

type ModuleStream = ModiStreamData<StreamData>;

struct PdbDebugInfo<'d> {
    pdb: Arc<Pdb<'d>>,
    addresses: Arc<AddressMap>,
    cpu_family: CpuFamily,
    types: types::Types,
    srcsrv: Option<srcsrv::OwnedSourceServerMappings>,
    modules: Vec<ModuleInfoFixed>,
    module_streams: FrozenMap<usize, Box<ModuleStream>>,
    module_names: HashMap<Vec<u8>, usize>,
}

impl<'d> PdbDebugInfo<'d> {
    fn build(object: &PdbObject<'d>) -> Result<Self, PdbError> {
        let pdb = &object.pdb;
        if pdb.named_stream("/names").is_some() {
            pdb.names()?;
        }
        let srcsrv = pdb
            .named_stream("srcsrv")
            .map(|index| pdb.read_stream(index))
            .transpose()?
            .and_then(|stream| SourceServerMappings::from_stream(stream).ok());
        let mut modules = Vec::new();
        let mut module_names = HashMap::new();
        let mut records = pdb.modules()?.iter();
        for module in records.by_ref() {
            module_names.insert(module.module_name().to_ascii_lowercase(), modules.len());
            modules.push(module.header().clone());
        }
        if !records.rest().is_empty() {
            return Err(anyhow::anyhow!("invalid PDB module list").into());
        }

        Ok(Self {
            pdb: pdb.clone(),
            addresses: object.addresses.clone(),
            cpu_family: object.arch().cpu_family(),
            types: types::Types::read(pdb)?,
            srcsrv,
            modules,
            module_streams: FrozenMap::new(),
            module_names,
        })
    }

    fn units(&self) -> PdbUnitIterator<'_> {
        PdbUnitIterator {
            debug_info: self,
            index: 0,
        }
    }

    fn module_stream(&self, index: usize) -> Result<Option<&ModuleStream>, PdbError> {
        if let Some(stream) = self.module_streams.get(&index) {
            return Ok(Some(stream));
        }
        let module = &self.modules[index];
        let Some(stream) = module.stream() else {
            return Ok(None);
        };
        let stream = ModuleStream::new(self.pdb.read_stream(stream)?, module)?;
        Ok(Some(self.module_streams.insert(index, Box::new(stream))))
    }

    fn resolve_index(&self, lines: &ModuleLines<'_>, index: TypeIndex) -> Option<TypeIndex> {
        if index.0 & 0x8000_0000 == 0 {
            return Some(index);
        }

        let (name_index, local) = lines.import(index.0)?;
        let name = self.pdb.get_name(NameIndex(name_index)).ok()?;
        let &module_index = self.module_names.get(&name.to_ascii_lowercase())?;
        let stream = self.module_stream(module_index).ok()??;
        lines::export(stream.c13_line_data(), local).map(TypeIndex)
    }

    fn file_info(&self, index: NameIndex) -> Result<FileInfo<'_>, PdbError> {
        let path = if self.pdb.named_stream("/names").is_some() {
            self.pdb.get_name(index)?
        } else {
            BStr::new(b"")
        };
        let mut file = FileInfo::from_path(path);
        if let Some(mappings) = &self.srcsrv {
            let original_path = file.path_str();
            if let Some(SourceServerInfo { path, revision }) =
                mappings.get().get_info(&original_path)
            {
                file.set_srcsrv_path(path.as_bytes());
                file.set_srcsrv_revision(revision);
            }
        }
        Ok(file)
    }
}

/// Debug session for PDB objects.
pub struct PdbDebugSession<'d> {
    debug_info: PdbDebugInfo<'d>,
}

impl<'d> PdbDebugSession<'d> {
    fn build(pdb: &PdbObject<'d>) -> Result<Self, PdbError> {
        Ok(PdbDebugSession {
            debug_info: PdbDebugInfo::build(pdb)?,
        })
    }

    /// Returns an iterator over all source files in this debug file.
    pub fn files(&self) -> PdbFileIterator<'_> {
        PdbFileIterator {
            debug_info: &self.debug_info,
            units: self.debug_info.units(),
            files: FileChecksumIter::new(&[]),
            finished: false,
        }
    }

    /// Returns an iterator over all functions in this debug file.
    pub fn functions(&self) -> PdbFunctionIterator<'_> {
        PdbFunctionIterator {
            units: self.debug_info.units(),
            functions: Vec::new().into_iter(),
            finished: false,
        }
    }

    /// Resolves a [`TypeRef`] in this debug file.
    ///
    /// See also: [`DebugSession::lookup_type`].
    pub fn lookup_type(&self, ty: &TypeRef) -> Result<Option<Type<'_>>, PdbError> {
        let Some(PdbTypeRef(index)) = ty.as_pdb() else {
            return Ok(None);
        };

        self.debug_info.types.lookup(*index)
    }

    /// See [DebugSession::source_by_path] for more information.
    pub fn source_by_path(
        &self,
        _path: &str,
    ) -> Result<Option<SourceFileDescriptor<'_>>, PdbError> {
        Ok(None)
    }

    /// Returns the SRCSRV VCS integration name if available.
    ///
    /// This extracts the version control system identifier from the SRCSRV stream,
    /// if present. Common values include "perforce", "tfs", "git", etc.
    /// Returns `None` if no SRCSRV stream exists or if it cannot be parsed.
    pub fn srcsrv_vcs_name(&self) -> Option<String> {
        self.debug_info
            .srcsrv
            .as_ref()
            .map(|srcsrv| srcsrv.get().vcs_name().to_owned())
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

    fn lookup_type(&'session self, ty: &TypeRef) -> Result<Option<Type<'session>>, Self::Error> {
        self.lookup_type(ty)
    }

    fn source_by_path(&self, path: &str) -> Result<Option<SourceFileDescriptor<'_>>, Self::Error> {
        self.source_by_path(path)
    }
}

struct Unit<'s> {
    debug_info: &'s PdbDebugInfo<'s>,
    module: &'s ModuleStream,
}

impl<'s> Unit<'s> {
    fn collect_lines(
        &self,
        module_lines: &ModuleLines<'_>,
        raw_lines: Vec<RawLine>,
    ) -> Result<Vec<LineInfo<'s>>, PdbError> {
        let mut lines = Vec::new();
        for line in raw_lines {
            let Some(address) = self.debug_info.addresses.rva(line.offset) else {
                continue;
            };
            if line.size == Some(0) {
                continue;
            }
            let file = self.debug_info.file_info(module_lines.file(line.file)?)?;
            lines.push(LineInfo {
                address: address.into(),
                size: line.size.map(u64::from),
                file,
                line: line.line.into(),
            });
        }
        lines.sort_by_key(|line| line.address);
        lines.dedup_by(|current, previous| {
            let end = previous
                .size
                .and_then(|size| previous.address.checked_add(size));
            if end == Some(current.address)
                && previous.file == current.file
                && previous.line == current.line
            {
                previous.size = previous
                    .size
                    .map(|size| size.saturating_add(current.size.unwrap_or(0)));
                return true;
            }
            false
        });
        Ok(lines)
    }

    fn function(
        &self,
        module_lines: &ModuleLines<'_>,
        offset: OffsetSegment,
        len: u32,
        proc: &Proc<'s>,
        kind: SymKind,
    ) -> Result<Option<Function<'s>>, PdbError> {
        let Some(address) = self.debug_info.addresses.rva(offset) else {
            return Ok(None);
        };
        let types = &self.debug_info.types;
        let resolve = |index| self.debug_info.resolve_index(module_lines, index);
        let index = resolve(proc.fixed.proc_type.get());
        let raw_name = String::from_utf8_lossy(proc.name);
        let name = index
            .and_then(|index| {
                let index = if matches!(
                    kind,
                    SymKind::S_GPROC32_ID | SymKind::S_LPROC32_ID | SymKind::S_LPROC32_DPC_ID
                ) {
                    types.function_type(index)?
                } else {
                    index
                };
                types.format_function(&raw_name, index, &resolve)
            })
            .map_or(raw_name, Cow::Owned);
        let lines = self.collect_lines(module_lines, module_lines.for_symbol(offset))?;
        Ok(Some(Function {
            address: address.into(),
            size: len.into(),
            name: Name::new(name, NameMangling::Unmangled, Language::Unknown),
            compilation_dir: &[],
            lines,
            inlinees: Vec::new(),
            variables: Vec::new(),
            inline: false,
        }))
    }

    fn inlinee(
        &self,
        module_lines: &ModuleLines<'_>,
        parent: OffsetSegment,
        id: u32,
        annotations: &[u8],
    ) -> Option<Function<'s>> {
        let raw_lines = module_lines.inlinee(parent, id, annotations)?;
        let lines = self.collect_lines(module_lines, raw_lines).ok()?;
        let address = lines.first()?.address;
        let last = lines.last()?;
        let end = last.address + last.size.unwrap_or(1);
        let index = self.debug_info.resolve_index(module_lines, TypeIndex(id))?;
        let name = self.debug_info.types.format_id(index, &|index| {
            self.debug_info.resolve_index(module_lines, index)
        })?;
        Some(Function {
            address,
            size: end - address,
            name: Name::new(name, NameMangling::Unmangled, Language::Unknown),
            compilation_dir: &[],
            lines,
            inlinees: Vec::new(),
            variables: Vec::new(),
            inline: true,
        })
    }

    fn functions(&self) -> Result<Vec<Function<'s>>, PdbError> {
        let module_lines = ModuleLines::new(self.module.c13_line_data_bytes())?;
        let bytes = self.module.full_sym_data()?;
        if !bytes.is_empty() && !bytes.starts_with(&CV_SIGNATURE_C13.to_le_bytes()) {
            return Err(anyhow::anyhow!("unsupported CodeView symbol data format").into());
        }
        let mut symbols = SymIter::for_module_syms(bytes);
        let mut frames = ProcedureFrames::new(bytes.len(), self.debug_info.cpu_family);
        let resolve_type_index = |index| {
            self.debug_info
                .resolve_index(&module_lines, index)
                .unwrap_or(index)
        };
        let mut depth = 0;
        let mut inc_next = false;
        let mut skipped_depth = None;
        let mut functions = Vec::new();
        let mut stack = FunctionStack::new(self.debug_info);
        let mut proc_offsets = SmallVec::<[_; 3]>::new();
        let mut last_proc = None;

        loop {
            let index = bytes.len() - symbols.rest().len();
            let Some(symbol) = symbols.next() else {
                break;
            };
            if inc_next {
                depth += 1;
            }
            inc_next = starts_scope(symbol.kind);
            if symbol.kind.ends_scope() {
                depth -= 1;
                if proc_offsets
                    .last()
                    .is_some_and(|&(scope, _)| scope >= depth)
                {
                    proc_offsets.pop();
                }
            }
            match skipped_depth {
                Some(skipped) if depth > skipped => continue,
                _ => skipped_depth = None,
            }
            if symbol.kind.ends_scope() {
                stack.flush(depth, &mut functions);
            }

            let data = parse_symbol(&symbol).ok();
            let builder = match data {
                Some(SymData::Proc(proc)) => {
                    let offset = proc.fixed.offset_segment;
                    proc_offsets.push((depth, offset));
                    let frame = frames.take(index, proc.fixed.p_end.get() as usize, &symbols);
                    let builder = self
                        .function(
                            &module_lines,
                            offset,
                            proc.fixed.proc_len.get(),
                            &proc,
                            symbol.kind,
                        )?
                        .map(|function| stack.standalone(depth, function, frame));
                    last_proc = Some((proc, frame, symbol.kind));
                    builder
                }
                _ if symbol.kind == SymKind::S_SEPCODE => {
                    let Some((proc, frame, kind)) = last_proc.as_ref() else {
                        continue;
                    };
                    let (offset, parent, len) = separated_code(symbol.data)?;
                    if proc.fixed.offset_segment != parent {
                        continue;
                    }
                    self.function(&module_lines, offset, len, proc, *kind)?
                        .map(|function| stack.standalone(depth, function, *frame))
                }
                data => {
                    let (id, annotations) = match data {
                        Some(SymData::InlineSite(site)) => {
                            (site.fixed.inlinee.get(), site.binary_annotations)
                        }
                        Some(SymData::InlineSite2(site)) => {
                            (site.fixed.inlinee.get(), site.binary_annotations)
                        }
                        Some(data) => {
                            if let Some(top) = stack.top_mut() {
                                top.visit_for_variables(depth, &symbol, &data, &resolve_type_index);
                            }
                            continue;
                        }
                        _ => continue,
                    };
                    let &(_, parent) = proc_offsets.last().ok_or(PdbErrorKind::UnexpectedInline)?;
                    self.inlinee(&module_lines, parent, id, annotations)
                        .map(|function| stack.inlinee(depth, function))
                }
            };
            match builder {
                Some(builder) => stack.push(builder),
                None => skipped_depth = Some(depth),
            }
        }
        if !symbols.rest().is_empty() {
            return Err(anyhow::anyhow!("truncated or invalid CodeView symbol record").into());
        }
        stack.flush(0, &mut functions);
        Ok(functions)
    }
}

fn parse_symbol<'s>(symbol: &Sym<'s>) -> Result<SymData<'s>, ParserError> {
    match symbol.kind {
        SymKind::S_LPROC32_ID
        | SymKind::S_GPROC32_ID
        | SymKind::S_LPROC32_DPC
        | SymKind::S_LPROC32_DPC_ID => symbol.parse_as::<Proc<'_>>().map(SymData::Proc),
        SymKind::S_LPROC32_ST | SymKind::S_GPROC32_ST => {
            let mut parser = Parser::new(symbol.data);
            Ok(SymData::Proc(Proc {
                fixed: parser.get()?,
                name: parser.strt_raw()?,
            }))
        }
        S_BLOCK32_ST => {
            let mut parser = Parser::new(symbol.data);
            Ok(SymData::Block(Block {
                fixed: parser.get()?,
                name: parser.strt_raw()?,
            }))
        }
        _ => symbol.parse(),
    }
}

fn starts_scope(kind: SymKind) -> bool {
    kind.starts_scope()
        || kind.is_proc()
        || matches!(
            kind,
            S_BLOCK32_ST | SymKind::S_WITH32 | S_WITH32_ST | S_THUNK32_ST
        )
}

fn separated_code(bytes: &[u8]) -> Result<(OffsetSegment, OffsetSegment, u32), ParserError> {
    let mut parser = Parser::new(bytes);
    parser.skip(8)?;
    let len = parser.u32()?;
    parser.skip(4)?;
    let offset = parser.u32()?;
    let parent = parser.u32()?;
    let segment = parser.u16()?;
    let parent_segment = parser.u16()?;
    Ok((
        OffsetSegment::new(offset, segment),
        OffsetSegment::new(parent, parent_segment),
        len,
    ))
}

struct PdbUnitIterator<'s> {
    debug_info: &'s PdbDebugInfo<'s>,
    index: usize,
}

impl<'s> Iterator for PdbUnitIterator<'s> {
    type Item = Result<Unit<'s>, PdbError>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.debug_info.modules.len() {
            let index = self.index;
            self.index += 1;
            match self.debug_info.module_stream(index) {
                Ok(Some(module)) => {
                    return Some(Ok(Unit {
                        debug_info: self.debug_info,
                        module,
                    }));
                }
                Ok(None) => continue,
                Err(error) => return Some(Err(error)),
            }
        }
        None
    }
}

/// An iterator over source files in a PDB object.
pub struct PdbFileIterator<'s> {
    debug_info: &'s PdbDebugInfo<'s>,
    units: PdbUnitIterator<'s>,
    files: FileChecksumIter<'s>,
    finished: bool,
}

impl<'s> Iterator for PdbFileIterator<'s> {
    type Item = Result<FileEntry<'s>, PdbError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        loop {
            if let Some(checksum) = self.files.next() {
                return Some(
                    self.debug_info
                        .file_info(checksum.name())
                        .map(|file| FileEntry::new(Cow::default(), file)),
                );
            }
            match self.units.next() {
                Some(Ok(unit)) => {
                    if let Some(checksums) = unit.module.c13_line_data().find_checksums() {
                        self.files = checksums.iter();
                    }
                }
                Some(Err(error)) => return Some(Err(error)),
                None => {
                    self.finished = true;
                    return None;
                }
            }
        }
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
