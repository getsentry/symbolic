//! Support for Mach Objects, used on macOS and iOS.

use std::borrow::Cow;
use std::error::Error;
use std::fmt;
use std::sync::Arc;

use goblin::mach;
use smallvec::SmallVec;
use thiserror::Error;

use symbolic_common::{Arch, AsSelf, CodeId, DebugId, Uuid};

use crate::base::*;
use crate::dwarf::{Dwarf, DwarfDebugSession, DwarfError, DwarfSection, Endian};
pub(crate) use mono_archive::{MonoArchive, MonoArchiveObjects};

mod bcsymbolmap;
pub mod compact;
mod mono_archive;

pub use bcsymbolmap::*;
pub use compact::*;

/// Prefix for hidden symbols from Apple BCSymbolMap builds.
const SWIFT_HIDDEN_PREFIX: &str = "__hidden#";

/// An error when dealing with [`MachObject`](struct.MachObject.html).
#[derive(Debug, Error)]
#[error("invalid MachO file")]
pub struct MachError {
    #[source]
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl MachError {
    /// Creates a new MachO error from an arbitrary error payload.
    fn new<E>(source: E) -> Self
    where
        E: Into<Box<dyn Error + Send + Sync>>,
    {
        let source = Some(source.into());
        Self { source }
    }
}

impl From<goblin::error::Error> for MachError {
    fn from(e: goblin::error::Error) -> Self {
        Self::new(e)
    }
}

impl From<scroll::Error> for MachError {
    fn from(e: scroll::Error) -> Self {
        Self::new(e)
    }
}

/// Mach Object containers, used for executables and debug companions on macOS and iOS.
pub struct MachObject<'d> {
    macho: mach::MachO<'d>,
    data: &'d [u8],
    bcsymbolmap: Option<Arc<BcSymbolMap<'d>>>,
}

impl<'d> MachObject<'d> {
    /// Tests whether the buffer could contain a MachO object.
    pub fn test(data: &[u8]) -> bool {
        matches!(MachArchive::is_fat(data), Some(false))
    }

    /// Tries to parse a MachO from the given slice.
    pub fn parse(data: &'d [u8]) -> Result<Self, MachError> {
        mach::MachO::parse(data, 0)
            .map(|macho| MachObject {
                macho,
                data,
                bcsymbolmap: None,
            })
            .map_err(MachError::new)
    }

    /// Parses and loads the [`BcSymbolMap`] into the object.
    ///
    /// The bitcode symbol map must match the object, there is nothing in the symbol map
    /// which allows this call to verify this.
    ///
    /// Once the symbolmap is loaded this object will transparently resolve any hidden
    /// symbols using the provided symbolmap.
    ///
    /// # Examples
    ///
    /// ```
    /// use symbolic_debuginfo::macho::{BcSymbolMap, MachObject};
    ///
    /// // let object_data = std::fs::read("dSYMs/.../Resources/DWARF/object").unwrap();
    /// # let object_data =
    /// #     std::fs::read("tests/fixtures/2d10c42f-591d-3265-b147-78ba0868073f.dwarf-hidden")
    /// #         .unwrap();
    /// let mut object = MachObject::parse(&object_data).unwrap();
    ///
    /// let map = object.symbol_map();
    /// let symbol = map.lookup(0x5a74).unwrap();
    /// assert_eq!(symbol.name.as_ref().map(|n| n.to_owned()).unwrap(), "__hidden#0_");
    ///
    /// // let bc_symbol_map_data =
    /// //     std::fs::read("BCSymbolMaps/c8374b6d-6e96-34d8-ae38-efaa5fec424f.bcsymbolmap")
    /// //     .unwrap();
    /// # let bc_symbol_map_data =
    /// #     std::fs::read("tests/fixtures/c8374b6d-6e96-34d8-ae38-efaa5fec424f.bcsymbolmap")
    /// #         .unwrap();
    /// let bc_symbol_map = BcSymbolMap::parse(&bc_symbol_map_data).unwrap();
    /// object.load_symbolmap(bc_symbol_map);
    ///
    ///
    /// let map = object.symbol_map();
    /// let symbol = map.lookup(0x5a74).unwrap();
    /// assert_eq!(
    ///     symbol.name.as_ref().map(|n| n.to_owned()).unwrap(),
    ///     "-[SentryMessage initWithFormatted:]",
    /// );
    /// ```
    // TODO: re-enable this deprecation once we have a convenient way of creating an owned SymCache Transformer.
    // #[deprecated = "use the symbolic-symcache `Transformer` functionality instead"]
    pub fn load_symbolmap(&mut self, symbolmap: BcSymbolMap<'d>) {
        self.bcsymbolmap = Some(Arc::new(symbolmap));
    }

    /// Gets the Compact Unwind Info of this object, if any exists.
    pub fn compact_unwind_info(&self) -> Result<Option<CompactUnwindInfoIter<'d>>, MachError> {
        if let Some(section) = self.section("unwind_info") {
            if let Cow::Borrowed(section) = section.data {
                let arch = self.arch();
                let is_little_endian = self.endianity() == Endian::Little;
                return Ok(Some(CompactUnwindInfoIter::new(
                    section,
                    is_little_endian,
                    arch,
                )?));
            }
        }
        Ok(None)
    }

    /// The container file format, which is always `FileFormat::MachO`.
    pub fn file_format(&self) -> FileFormat {
        FileFormat::MachO
    }

    fn find_uuid(&self) -> Option<Uuid> {
        for cmd in &self.macho.load_commands {
            if let mach::load_command::CommandVariant::Uuid(ref uuid_cmd) = cmd.command {
                return Uuid::from_slice(&uuid_cmd.uuid).ok();
            }
        }

        None
    }

    /// The name of the dylib if any.
    pub fn name(&self) -> Option<&'d str> {
        self.macho.name
    }

    /// The code identifier of this object.
    ///
    /// Mach objects use a UUID which is specified in the load commands that are part of the Mach
    /// header. This UUID is generated at compile / link time and is usually unique per compilation.
    pub fn code_id(&self) -> Option<CodeId> {
        let uuid = self.find_uuid()?;
        Some(CodeId::from_binary(&uuid.as_bytes()[..]))
    }

    /// The debug information identifier of a MachO file.
    ///
    /// This uses the same UUID as `code_id`.
    pub fn debug_id(&self) -> DebugId {
        self.find_uuid().map(DebugId::from_uuid).unwrap_or_default()
    }

    /// The CPU architecture of this object, as specified in the Mach header.
    pub fn arch(&self) -> Arch {
        use goblin::mach::constants::cputype;

        match (self.macho.header.cputype(), self.macho.header.cpusubtype()) {
            (cputype::CPU_TYPE_I386, cputype::CPU_SUBTYPE_I386_ALL) => Arch::X86,
            (cputype::CPU_TYPE_I386, _) => Arch::X86Unknown,
            (cputype::CPU_TYPE_X86_64, cputype::CPU_SUBTYPE_X86_64_ALL) => Arch::Amd64,
            (cputype::CPU_TYPE_X86_64, cputype::CPU_SUBTYPE_X86_64_H) => Arch::Amd64h,
            (cputype::CPU_TYPE_X86_64, _) => Arch::Amd64Unknown,
            (cputype::CPU_TYPE_ARM64, cputype::CPU_SUBTYPE_ARM64_ALL) => Arch::Arm64,
            (cputype::CPU_TYPE_ARM64, cputype::CPU_SUBTYPE_ARM64_V8) => Arch::Arm64V8,
            (cputype::CPU_TYPE_ARM64, cputype::CPU_SUBTYPE_ARM64_E) => Arch::Arm64e,
            (cputype::CPU_TYPE_ARM64, _) => Arch::Arm64Unknown,
            (cputype::CPU_TYPE_ARM64_32, cputype::CPU_SUBTYPE_ARM64_32_ALL) => Arch::Arm64_32,
            (cputype::CPU_TYPE_ARM64_32, cputype::CPU_SUBTYPE_ARM64_32_V8) => Arch::Arm64_32V8,
            (cputype::CPU_TYPE_ARM64_32, _) => Arch::Arm64_32Unknown,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_ALL) => Arch::Arm,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V5TEJ) => Arch::ArmV5,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V6) => Arch::ArmV6,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V6M) => Arch::ArmV6m,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V7) => Arch::ArmV7,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V7F) => Arch::ArmV7f,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V7S) => Arch::ArmV7s,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V7K) => Arch::ArmV7k,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V7M) => Arch::ArmV7m,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V7EM) => Arch::ArmV7em,
            (cputype::CPU_TYPE_ARM, _) => Arch::ArmUnknown,
            (cputype::CPU_TYPE_POWERPC, cputype::CPU_SUBTYPE_POWERPC_ALL) => Arch::Ppc,
            (cputype::CPU_TYPE_POWERPC64, cputype::CPU_SUBTYPE_POWERPC_ALL) => Arch::Ppc64,
            (_, _) => Arch::Unknown,
        }
    }

    /// The kind of this object, as specified in the Mach header.
    pub fn kind(&self) -> ObjectKind {
        match self.macho.header.filetype {
            goblin::mach::header::MH_OBJECT => ObjectKind::Relocatable,
            goblin::mach::header::MH_EXECUTE => ObjectKind::Executable,
            goblin::mach::header::MH_FVMLIB => ObjectKind::Library,
            goblin::mach::header::MH_CORE => ObjectKind::Dump,
            goblin::mach::header::MH_PRELOAD => ObjectKind::Executable,
            goblin::mach::header::MH_DYLIB => ObjectKind::Library,
            goblin::mach::header::MH_DYLINKER => ObjectKind::Executable,
            goblin::mach::header::MH_BUNDLE => ObjectKind::Library,
            goblin::mach::header::MH_DSYM => ObjectKind::Debug,
            goblin::mach::header::MH_KEXT_BUNDLE => ObjectKind::Library,
            _ => ObjectKind::Other,
        }
    }

    /// The address at which the image prefers to be loaded into memory.
    ///
    /// MachO files store all internal addresses as if it was loaded at that address. When the image
    /// is actually loaded, that spot might already be taken by other images and so it must be
    /// relocated to a new address. At runtime, a relocation table manages the arithmetics behind
    /// this.
    ///
    /// Addresses used in `symbols` or `debug_session` have already been rebased relative to that
    /// load address, so that the caller only has to deal with addresses relative to the actual
    /// start of the image.
    pub fn load_address(&self) -> u64 {
        for seg in &self.macho.segments {
            if seg.name().map(|name| name == "__TEXT").unwrap_or(false) {
                return seg.vmaddr;
            }
        }

        0
    }

    /// Determines whether this object exposes a public symbol table.
    pub fn has_symbols(&self) -> bool {
        self.macho.symbols.is_some()
    }

    /// Returns an iterator over symbols in the public symbol table.
    pub fn symbols(&self) -> MachOSymbolIterator<'d> {
        // Cache indices of code sections. These are either "__text" or "__stubs", always located in
        // the "__TEXT" segment. It looks like each of those sections only occurs once, but to be
        // safe they are collected into a vector.
        let mut sections = SmallVec::new();
        let mut section_index = 0;

        'outer: for segment in &self.macho.segments {
            if segment.name().ok() != Some("__TEXT") {
                section_index += segment.nsects as usize;
                continue;
            }

            for result in segment {
                // Do not continue to iterate potentially broken section headers. This could lead to
                // invalid section indices.
                let section = match result {
                    Ok((section, _data)) => section,
                    Err(_) => break 'outer,
                };

                match section.name() {
                    Ok("__text") | Ok("__stubs") => sections.push(section_index),
                    _ => (),
                }

                section_index += 1;
            }
        }

        MachOSymbolIterator {
            symbols: self.macho.symbols(),
            sections,
            vmaddr: self.load_address(),
            symbolmap: self.bcsymbolmap.clone(),
        }
    }

    /// Returns an ordered map of symbols in the symbol table.
    pub fn symbol_map(&self) -> SymbolMap<'d> {
        self.symbols().collect()
    }

    /// Determines whether this object contains debug information.
    pub fn has_debug_info(&self) -> bool {
        self.has_section("debug_info")
    }

    /// Constructs a debugging session.
    ///
    /// A debugging session loads certain information from the object file and creates caches for
    /// efficient access to various records in the debug information. Since this can be quite a
    /// costly process, try to reuse the debugging session as long as possible.
    ///
    /// MachO files generally use DWARF debugging information, which is also used by ELF containers
    /// on Linux.
    ///
    /// Constructing this session will also work if the object does not contain debugging
    /// information, in which case the session will be a no-op. This can be checked via
    /// [`has_debug_info`](struct.MachObject.html#method.has_debug_info).
    pub fn debug_session(&self) -> Result<DwarfDebugSession<'d>, DwarfError> {
        let symbols = self.symbol_map();
        let mut session =
            DwarfDebugSession::parse(self, symbols, self.load_address() as i64, self.kind())?;
        session.load_symbolmap(self.bcsymbolmap.clone());
        Ok(session)
    }

    /// Determines whether this object contains stack unwinding information.
    pub fn has_unwind_info(&self) -> bool {
        self.has_section("eh_frame")
            || self.has_section("debug_frame")
            || self.has_section("unwind_info")
    }

    /// Determines whether this object contains embedded source.
    pub fn has_sources(&self) -> bool {
        false
    }

    /// Determines whether this object is malformed and was only partially parsed
    pub fn is_malformed(&self) -> bool {
        false
    }

    /// Returns the raw data of the ELF file.
    pub fn data(&self) -> &'d [u8] {
        self.data
    }

    /// Checks whether this mach object contains hidden symbols.
    ///
    /// This is an indication that BCSymbolMaps are needed to symbolicate crash reports correctly.
    pub fn requires_symbolmap(&self) -> bool {
        self.symbols().any(|s| {
            s.name()
                .map_or(false, |n| n.starts_with(SWIFT_HIDDEN_PREFIX))
        })
    }
}

impl fmt::Debug for MachObject<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MachObject")
            .field("code_id", &self.code_id())
            .field("debug_id", &self.debug_id())
            .field("arch", &self.arch())
            .field("kind", &self.kind())
            .field("load_address", &format_args!("{:#x}", self.load_address()))
            .field("has_symbols", &self.has_symbols())
            .field("has_debug_info", &self.has_debug_info())
            .field("has_unwind_info", &self.has_unwind_info())
            .field("is_malformed", &self.is_malformed())
            .finish()
    }
}

impl<'slf, 'd: 'slf> AsSelf<'slf> for MachObject<'d> {
    type Ref = MachObject<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        self
    }
}

impl<'d> Parse<'d> for MachObject<'d> {
    type Error = MachError;

    fn test(data: &[u8]) -> bool {
        Self::test(data)
    }

    fn parse(data: &'d [u8]) -> Result<Self, MachError> {
        Self::parse(data)
    }
}

impl<'data: 'object, 'object> ObjectLike<'data, 'object> for MachObject<'data> {
    type Error = DwarfError;
    type Session = DwarfDebugSession<'data>;
    type SymbolIterator = MachOSymbolIterator<'data>;

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

    fn symbols(&self) -> Self::SymbolIterator {
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

impl<'data> Dwarf<'data> for MachObject<'data> {
    fn endianity(&self) -> Endian {
        if self.macho.little_endian {
            Endian::Little
        } else {
            Endian::Big
        }
    }

    fn raw_section(&self, section_name: &str) -> Option<DwarfSection<'data>> {
        for segment in &self.macho.segments {
            for section in segment.into_iter() {
                let (header, data) = section.ok()?;
                if let Ok(sec) = header.name() {
                    if sec.starts_with("__") && &sec[2..] == section_name {
                        // In some cases, dsymutil leaves sections headers but removes their
                        // data from the file. While the addr and size parameters are still
                        // set, `header.offset` is 0 in that case. We skip them just like the
                        // section was missing to avoid loading invalid data.
                        if header.offset == 0 {
                            return None;
                        }

                        return Some(DwarfSection {
                            data: Cow::Borrowed(data),
                            address: header.addr,
                            offset: u64::from(header.offset),
                            align: u64::from(header.align),
                        });
                    }
                }
            }
        }

        None
    }
}

/// An iterator over symbols in the MachO file.
///
/// Returned by [`MachObject::symbols`](struct.MachObject.html#method.symbols).
pub struct MachOSymbolIterator<'data> {
    symbols: mach::symbols::SymbolIterator<'data>,
    sections: SmallVec<[usize; 2]>,
    vmaddr: u64,
    symbolmap: Option<Arc<BcSymbolMap<'data>>>,
}

impl<'data> Iterator for MachOSymbolIterator<'data> {
    type Item = Symbol<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        for next in &mut self.symbols {
            let (mut name, nlist) = next.ok()?;

            // Sanity check of the symbol address. Since we only intend to iterate over function
            // symbols, they need to be mapped after the image's vmaddr.
            if nlist.n_value < self.vmaddr {
                continue;
            }

            // We are only interested in symbols pointing to a code section (type `N_SECT`). The
            // section index is incremented by one to leave room for `NO_SECT` (0). Section indexes
            // of the code sections have been passed in via `self.sections`.
            let in_valid_section = !nlist.is_stab()
                && nlist.get_type() == mach::symbols::N_SECT
                && nlist.n_sect != (mach::symbols::NO_SECT as usize)
                && self.sections.contains(&(nlist.n_sect - 1));

            if !in_valid_section {
                continue;
            }

            if let Some(symbolmap) = self.symbolmap.as_ref() {
                name = symbolmap.resolve(name);
            }

            // Trim leading underscores from mangled C++ names.
            if let Some(tail) = name.strip_prefix('_') {
                if !name.starts_with(SWIFT_HIDDEN_PREFIX) {
                    name = tail;
                }
            }

            return Some(Symbol {
                name: Some(Cow::Borrowed(name)),
                address: nlist.n_value - self.vmaddr,
                size: 0, // Computed in `SymbolMap`
            });
        }

        None
    }
}

/// An iterator over objects in a [`FatMachO`](struct.FatMachO.html).
///
/// Objects are parsed just-in-time while iterating, which may result in errors. The iterator is
/// still valid afterwards, however, and can be used to resolve the next object.
pub struct FatMachObjectIterator<'d, 'a> {
    iter: mach::FatArchIterator<'a>,
    remaining: usize,
    data: &'d [u8],
}

impl<'d, 'a> Iterator for FatMachObjectIterator<'d, 'a> {
    type Item = Result<MachObject<'d>, MachError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        self.remaining -= 1;
        match self.iter.next() {
            Some(Ok(arch)) => {
                let start = (arch.offset as usize).min(self.data.len());
                let end = (arch.offset as usize + arch.size as usize).min(self.data.len());
                Some(MachObject::parse(&self.data[start..end]))
            }
            Some(Err(error)) => Some(Err(MachError::new(error))),
            None => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl std::iter::FusedIterator for FatMachObjectIterator<'_, '_> {}
impl ExactSizeIterator for FatMachObjectIterator<'_, '_> {}

/// A fat MachO container that hosts one or more [`MachObject`]s.
///
/// [`MachObject`]: struct.MachObject.html
pub struct FatMachO<'d> {
    fat: mach::MultiArch<'d>,
    data: &'d [u8],
}

impl<'d> FatMachO<'d> {
    /// Tests whether the buffer could contain an ELF object.
    pub fn test(data: &[u8]) -> bool {
        matches!(MachArchive::is_fat(data), Some(true))
    }

    /// Tries to parse a fat MachO container from the given slice.
    pub fn parse(data: &'d [u8]) -> Result<Self, MachError> {
        mach::MultiArch::new(data)
            .map(|fat| FatMachO { fat, data })
            .map_err(MachError::new)
    }

    /// Returns an iterator over objects in this container.
    pub fn objects(&self) -> FatMachObjectIterator<'d, '_> {
        FatMachObjectIterator {
            iter: self.fat.iter_arches(),
            remaining: self.fat.narches,
            data: self.data,
        }
    }

    /// Returns the number of objects in this archive.
    pub fn object_count(&self) -> usize {
        self.fat.narches
    }

    /// Resolves the object at the given index.
    ///
    /// Returns `Ok(None)` if the index is out of bounds, or `Err` if the object exists but cannot
    /// be parsed.
    pub fn object_by_index(&self, index: usize) -> Result<Option<MachObject<'d>>, MachError> {
        let arch = match self.fat.iter_arches().nth(index) {
            Some(arch) => arch.map_err(MachError::new)?,
            None => return Ok(None),
        };

        let start = (arch.offset as usize).min(self.data.len());
        let end = (arch.offset as usize + arch.size as usize).min(self.data.len());
        MachObject::parse(&self.data[start..end]).map(Some)
    }
}

impl fmt::Debug for FatMachO<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FatMachO").field("fat", &self.fat).finish()
    }
}

impl<'slf, 'd: 'slf> AsSelf<'slf> for FatMachO<'d> {
    type Ref = FatMachO<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        self
    }
}

#[allow(clippy::large_enum_variant)]
enum MachObjectIteratorInner<'d, 'a> {
    Single(MonoArchiveObjects<'d, MachObject<'d>>),
    Archive(FatMachObjectIterator<'d, 'a>),
}

/// An iterator over objects in a [`MachArchive`](struct.MachArchive.html).
pub struct MachObjectIterator<'d, 'a>(MachObjectIteratorInner<'d, 'a>);

impl<'d, 'a> Iterator for MachObjectIterator<'d, 'a> {
    type Item = Result<MachObject<'d>, MachError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.0 {
            MachObjectIteratorInner::Single(ref mut iter) => iter.next(),
            MachObjectIteratorInner::Archive(ref mut iter) => iter.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self.0 {
            MachObjectIteratorInner::Single(ref iter) => iter.size_hint(),
            MachObjectIteratorInner::Archive(ref iter) => iter.size_hint(),
        }
    }
}

impl std::iter::FusedIterator for MachObjectIterator<'_, '_> {}
impl ExactSizeIterator for MachObjectIterator<'_, '_> {}

#[derive(Debug)]
enum MachArchiveInner<'d> {
    Single(MonoArchive<'d, MachObject<'d>>),
    Archive(FatMachO<'d>),
}

/// An archive that can consist of a single [`MachObject`] or a [`FatMachO`] container.
///
/// Executables and dSYM files on macOS can be a so-called _Fat Mach Object_: It contains multiple
/// objects for several architectures. When loading this object, the operating system determines the
/// object corresponding to the host's architecture. This allows to distribute a single binary with
/// optimizations for specific CPUs, which is frequently done on iOS.
///
/// To abstract over the differences, `MachArchive` simulates the archive interface also for single
/// Mach objects. This allows uniform access to both file types.
///
/// [`MachObject`]: struct.MachObject.html
/// [`FatMachO`]: struct.FatMachO.html
#[derive(Debug)]
pub struct MachArchive<'d>(MachArchiveInner<'d>);

impl<'d> MachArchive<'d> {
    /// Tests whether the buffer contains either a Mach Object or a Fat Mach Object.
    pub fn test(data: &[u8]) -> bool {
        Self::is_fat(data).is_some()
    }

    /// Determines if the binary content is a macho object, and whether or not it is fat
    fn is_fat(data: &[u8]) -> Option<bool> {
        let (magic, _maybe_ctx) = goblin::mach::parse_magic_and_ctx(data, 0).ok()?;
        match magic {
            goblin::mach::fat::FAT_MAGIC => {
                use scroll::Pread;
                // so this is kind of stupid but java class files share the same cutesy magic
                // as a macho fat file (CAFEBABE).  This means that we often claim that a java
                // class file is actually a macho binary but it's not.  The next 32 bits encode
                // the number of embedded architectures in a fat mach.  In case of a JAR file
                // we have 2 bytes for minor version and 2 bytes for major version of the class
                // file format.
                //
                // The internet suggests the first public version of Java had the class version
                // 45.  Thus the logic applied here is that if the number is >= 45 we're more
                // likely to have a java class file than a macho file with 45 architectures
                // which should be very rare.
                //
                // https://docs.oracle.com/javase/specs/jvms/se6/html/ClassFile.doc.html
                let narches = data.pread_with::<u32>(4, scroll::BE).ok()?;

                if narches < 45 {
                    Some(true)
                } else {
                    None
                }
            }
            goblin::mach::header::MH_CIGAM_64
            | goblin::mach::header::MH_CIGAM
            | goblin::mach::header::MH_MAGIC_64
            | goblin::mach::header::MH_MAGIC => Some(false),
            _ => None,
        }
    }

    /// Tries to parse a Mach archive from the given slice.
    pub fn parse(data: &'d [u8]) -> Result<Self, MachError> {
        Ok(Self(match Self::is_fat(data) {
            Some(true) => MachArchiveInner::Archive(FatMachO::parse(data)?),
            // Fall back to mach parsing to receive a meaningful error message from goblin
            _ => MachArchiveInner::Single(MonoArchive::new(data)),
        }))
    }

    /// Returns an iterator over all objects contained in this archive.
    pub fn objects(&self) -> MachObjectIterator<'d, '_> {
        MachObjectIterator(match self.0 {
            MachArchiveInner::Single(ref inner) => MachObjectIteratorInner::Single(inner.objects()),
            MachArchiveInner::Archive(ref inner) => {
                MachObjectIteratorInner::Archive(inner.objects())
            }
        })
    }

    /// Returns the number of objects in this archive.
    pub fn object_count(&self) -> usize {
        match self.0 {
            MachArchiveInner::Single(ref inner) => inner.object_count(),
            MachArchiveInner::Archive(ref inner) => inner.object_count(),
        }
    }

    /// Resolves the object at the given index.
    ///
    /// Returns `Ok(None)` if the index is out of bounds, or `Err` if the object exists but cannot
    /// be parsed.
    pub fn object_by_index(&self, index: usize) -> Result<Option<MachObject<'d>>, MachError> {
        match self.0 {
            MachArchiveInner::Single(ref inner) => inner.object_by_index(index),
            MachArchiveInner::Archive(ref inner) => inner.object_by_index(index),
        }
    }

    /// Returns whether this is a multi-object archive.
    ///
    /// This may also return true if there is only a single object inside the archive.
    pub fn is_multi(&self) -> bool {
        match self.0 {
            MachArchiveInner::Archive(_) => true,
            MachArchiveInner::Single(_) => false,
        }
    }
}

impl<'slf, 'd: 'slf> AsSelf<'slf> for MachArchive<'d> {
    type Ref = MachArchive<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bcsymbolmap() {
        let object_data =
            std::fs::read("tests/fixtures/2d10c42f-591d-3265-b147-78ba0868073f.dwarf-hidden")
                .unwrap();
        let mut object = MachObject::parse(&object_data).unwrap();

        // make sure that we get hidden symbols/filenames before loading the symbolmap
        let mut symbols = object.symbols();
        let symbol = symbols.next().unwrap();
        assert_eq!(symbol.name.unwrap(), "__hidden#0_");

        let session = object.debug_session().unwrap();
        let mut files = session.files();
        let file = files.next().unwrap().unwrap();
        assert_eq!(&file.path_str(), "__hidden#41_/__hidden#42_");
        assert_eq!(
            &file.abs_path_str(),
            // XXX: the path joining logic usually detects absolute paths (see below), but that does
            // not work for these hidden paths.
            "__hidden#41_/__hidden#41_/__hidden#42_"
        );

        let mut functions = session.functions();
        let function = functions.next().unwrap().unwrap();
        assert_eq!(&function.name, "__hidden#0_");
        assert_eq!(&function.compilation_dir, b"__hidden#41_");
        assert_eq!(
            &function.lines[0].file.path_str(),
            "__hidden#41_/__hidden#42_"
        );

        let fn_with_inlinees = functions
            .filter_map(|f| f.ok())
            .find(|f| !f.inlinees.is_empty())
            .unwrap();
        let inlinee = fn_with_inlinees.inlinees.first().unwrap();
        assert_eq!(&inlinee.name, "__hidden#146_");

        // loads the symbolmap
        let bc_symbol_map_data =
            std::fs::read("tests/fixtures/c8374b6d-6e96-34d8-ae38-efaa5fec424f.bcsymbolmap")
                .unwrap();
        let bc_symbol_map = BcSymbolMap::parse(&bc_symbol_map_data).unwrap();
        object.load_symbolmap(bc_symbol_map);

        // make sure we get resolved symbols/filenames now
        let mut symbols = object.symbols();
        let symbol = symbols.next().unwrap();
        assert_eq!(symbol.name.unwrap(), "-[SentryMessage initWithFormatted:]");

        let symbol = symbols.next().unwrap();
        assert_eq!(symbol.name.unwrap(), "-[SentryMessage setMessage:]");

        let session = object.debug_session().unwrap();
        let mut files = session.files();
        let file = files.next().unwrap().unwrap();
        assert_eq!(
            &file.path_str(),
            "/Users/philipphofmann/git-repos/sentry-cocoa/Sources/Sentry/SentryMessage.m"
        );
        assert_eq!(
            &file.abs_path_str(),
            "/Users/philipphofmann/git-repos/sentry-cocoa/Sources/Sentry/SentryMessage.m"
        );

        let mut functions = session.functions();
        let function = functions.next().unwrap().unwrap();
        assert_eq!(&function.name, "-[SentryMessage initWithFormatted:]");
        assert_eq!(
            &function.compilation_dir,
            b"/Users/philipphofmann/git-repos/sentry-cocoa"
        );
        assert_eq!(
            &function.lines[0].file.path_str(),
            "/Users/philipphofmann/git-repos/sentry-cocoa/Sources/Sentry/SentryMessage.m"
        );

        let fn_with_inlinees = functions
            .filter_map(|f| f.ok())
            .find(|f| !f.inlinees.is_empty())
            .unwrap();
        let inlinee = fn_with_inlinees.inlinees.first().unwrap();
        assert_eq!(&inlinee.name, "prepareReportWriter");
    }

    #[test]
    fn test_overflow_multiarch() {
        let data = [
            0xbe, 0xba, 0xfe, 0xca, // magic
            0x00, 0x00, 0x00, 0x01, // num arches = 1
            0x00, 0x00, 0x00, 0x00, // cpu type
            0x00, 0x00, 0x00, 0x00, // cpu subtype
            0x00, 0xff, 0xff, 0xff, // offset
            0x00, 0x00, 0xff, 0xff, // size
            0x00, 0x00, 0x00, 0x00, // align
        ];

        let fat = FatMachO::parse(&data).unwrap();

        let obj = fat.object_by_index(0);
        assert!(obj.is_err());

        let mut iter = fat.objects();
        assert!(iter.next().unwrap().is_err());
    }

    #[test]
    fn test_section_access() {
        let data = [
            0xfe, 0xed, 0xfa, 0xcf, 0x1, 0x0, 0x0, 0x0, 0x0, 0x2, 0xed, 0xfa, 0xce, 0x6f, 0x73,
            0x6f, 0x0, 0x0, 0x0, 0x7, 0x0, 0x0, 0x0, 0x4d, 0x4f, 0x44, 0x55, 0x4c, 0x40, 0x20, 0x0,
            0x0, 0x0, 0x0, 0x0, 0x1, 0x0, 0x0, 0x0, 0x0, 0x0, 0x3, 0x4d, 0xc2, 0xc2, 0xc2, 0xc2,
            0xc2, 0xc2, 0xc2, 0xc2, 0xc2, 0xc2, 0xca, 0x7a, 0xfe, 0xba, 0xbe, 0x0, 0x0, 0x0, 0x20,
            0x43, 0x2f, 0x0, 0x32, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x7, 0x0, 0x0, 0x0, 0x4d, 0x4f,
            0x44, 0x55, 0x4c, 0x40, 0x20, 0x0, 0x0, 0x0, 0x0, 0x0, 0x1, 0x0, 0x0, 0x0, 0x0, 0x0,
            0x0, 0x0, 0x2a, 0x78, 0x6e, 0xc2, 0xc2, 0xc2, 0xc2, 0xc2, 0xc2, 0xc2, 0xc2, 0xc2, 0xc2,
            0xc2, 0xc2, 0xc2, 0xc2, 0xc6, 0xd5, 0xc2, 0xc2, 0x1f, 0x1f,
        ];

        let obj = MachObject::parse(&data).unwrap();

        assert!(!obj.has_debug_info());
    }

    #[test]
    fn test_invalid_symbols() {
        let data = std::fs::read("tests/fixtures/invalid-symbols.fuzzed").unwrap();

        let obj = MachObject::parse(&data).unwrap();

        let _ = obj.symbol_map();
    }
}
