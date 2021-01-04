//! Support for the Executable and Linkable Format, used on Linux.

use std::borrow::Cow;
use std::error::Error;
use std::fmt;
use std::io::Cursor;

use flate2::{Decompress, FlushDecompress};
use goblin::elf::compression_header::{CompressionHeader, ELFCOMPRESS_ZLIB};
use goblin::{container::Ctx, elf, strtab};
use thiserror::Error;

use symbolic_common::{Arch, AsSelf, CodeId, DebugId, Uuid};

use crate::base::*;
use crate::dwarf::{Dwarf, DwarfDebugSession, DwarfError, DwarfSection, Endian};
use crate::private::Parse;

const UUID_SIZE: usize = 16;
const PAGE_SIZE: usize = 4096;

const SHN_UNDEF: usize = elf::section_header::SHN_UNDEF as usize;
const SHF_COMPRESSED: u64 = elf::section_header::SHF_COMPRESSED as u64;

/// This file follows the first MIPS 32 bit ABI
#[allow(unused)]
const EF_MIPS_ABI_O32: u32 = 0x0000_1000;
/// O32 ABI extended for 64-bit architecture.
const EF_MIPS_ABI_O64: u32 = 0x0000_2000;
/// EABI in 32 bit mode.
#[allow(unused)]
const EF_MIPS_ABI_EABI32: u32 = 0x0000_3000;
/// EABI in 64 bit mode.
const EF_MIPS_ABI_EABI64: u32 = 0x0000_4000;

/// Any flag value that might indicate 64-bit MIPS.
const MIPS_64_FLAGS: u32 = EF_MIPS_ABI_O64 | EF_MIPS_ABI_EABI64;

/// An error when dealing with [`ElfObject`](struct.ElfObject.html).
#[derive(Debug, Error)]
#[error("invalid ELF file")]
pub struct ElfError {
    #[source]
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl ElfError {
    /// Creates a new ELF error from an arbitrary error payload.
    fn new<E>(source: E) -> Self
    where
        E: Into<Box<dyn Error + Send + Sync>>,
    {
        let source = Some(source.into());
        Self { source }
    }
}

/// Executable and Linkable Format, used for executables and libraries on Linux.
pub struct ElfObject<'data> {
    elf: elf::Elf<'data>,
    data: &'data [u8],
}

impl<'data> ElfObject<'data> {
    /// Tests whether the buffer could contain an ELF object.
    pub fn test(data: &[u8]) -> bool {
        matches!(
            goblin::peek(&mut Cursor::new(data)),
            Ok(goblin::Hint::Elf(_))
        )
    }

    /// Tries to parse an ELF object from the given slice.
    pub fn parse(data: &'data [u8]) -> Result<Self, ElfError> {
        elf::Elf::parse(data)
            .map(|elf| ElfObject { elf, data })
            .map_err(ElfError::new)
    }

    /// The container file format, which is always `FileFormat::Elf`.
    pub fn file_format(&self) -> FileFormat {
        FileFormat::Elf
    }

    /// The code identifier of this object.
    ///
    /// As opposed to Mach-O, ELF does not specify a unique ID for object files in
    /// its header. Compilers and linkers usually add either `SHT_NOTE` sections or
    /// `PT_NOTE` program header elements for this purpose.
    pub fn code_id(&self) -> Option<CodeId> {
        self.find_build_id()
            .filter(|slice| !slice.is_empty())
            .map(|slice| CodeId::from_binary(slice))
    }

    /// The binary's soname, if any.
    pub fn name(&self) -> Option<&'data str> {
        self.elf.soname
    }

    /// The debug information identifier of an ELF object.
    ///
    /// The debug identifier is a rehash of the first 16 bytes of the `code_id`, if
    /// present. Otherwise, this function will hash the first page of the `.text`
    /// section (program code) to synthesize a unique ID. This is likely not a valid
    /// UUID since was generated off a hash value.
    ///
    /// If all of the above fails, the identifier will be an empty `DebugId`.
    pub fn debug_id(&self) -> DebugId {
        // Search for a GNU build identifier node in the program headers or the
        // build ID section. If errors occur during this process, fall through
        // silently to the next method.
        if let Some(identifier) = self.find_build_id() {
            return self.compute_debug_id(identifier);
        }

        // We were not able to locate the build ID, so fall back to hashing the
        // first page of the ".text" (program code) section. This algorithm XORs
        // 16-byte chunks directly into a UUID buffer.
        if let Some(section) = self.raw_section("text") {
            let mut hash = [0; UUID_SIZE];
            for i in 0..std::cmp::min(section.data.len(), PAGE_SIZE) {
                hash[i % UUID_SIZE] ^= section.data[i];
            }

            return self.compute_debug_id(&hash);
        }

        DebugId::default()
    }

    /// The CPU architecture of this object, as specified in the ELF header.
    pub fn arch(&self) -> Arch {
        match self.elf.header.e_machine {
            goblin::elf::header::EM_386 => Arch::X86,
            goblin::elf::header::EM_X86_64 => Arch::Amd64,
            goblin::elf::header::EM_AARCH64 => Arch::Arm64,
            // NOTE: This could actually be any of the other 32bit ARMs. Since we don't need this
            // information, we use the generic Arch::Arm. By reading CPU_arch and FP_arch attributes
            // from the SHT_ARM_ATTRIBUTES section it would be possible to distinguish the ARM arch
            // version and infer hard/soft FP.
            //
            // For more information, see:
            // http://code.metager.de/source/xref/gnu/src/binutils/readelf.c#11282
            // https://stackoverflow.com/a/20556156/4228225
            goblin::elf::header::EM_ARM => Arch::Arm,
            goblin::elf::header::EM_PPC => Arch::Ppc,
            goblin::elf::header::EM_PPC64 => Arch::Ppc64,
            goblin::elf::header::EM_MIPS | goblin::elf::header::EM_MIPS_RS3_LE => {
                if self.elf.header.e_flags & MIPS_64_FLAGS != 0 {
                    Arch::Mips64
                } else {
                    Arch::Mips
                }
            }
            _ => Arch::Unknown,
        }
    }

    /// The kind of this object, as specified in the ELF header.
    pub fn kind(&self) -> ObjectKind {
        let kind = match self.elf.header.e_type {
            goblin::elf::header::ET_NONE => ObjectKind::None,
            goblin::elf::header::ET_REL => ObjectKind::Relocatable,
            goblin::elf::header::ET_EXEC => ObjectKind::Executable,
            goblin::elf::header::ET_DYN => ObjectKind::Library,
            goblin::elf::header::ET_CORE => ObjectKind::Dump,
            _ => ObjectKind::Other,
        };

        // When stripping debug information into a separate file with objcopy,
        // the eh_type field still reads ET_EXEC. However, the interpreter is
        // removed. Since an executable without interpreter does not make any
        // sense, we assume ``Debug`` in this case.
        if kind == ObjectKind::Executable && self.elf.interpreter.is_none() {
            return ObjectKind::Debug;
        }

        // The same happens for libraries. However, here we can only check for
        // a missing text section. If this still yields too many false positivies,
        // we will have to check either the size or offset of that section in
        // the future.
        if kind == ObjectKind::Library && self.raw_section("text").is_none() {
            return ObjectKind::Debug;
        }

        kind
    }

    /// The address at which the image prefers to be loaded into memory.
    ///
    /// ELF files store all internal addresses as if it was loaded at that address. When the image
    /// is actually loaded, that spot might already be taken by other images and so it must be
    /// relocated to a new address. At runtime, a relocation table manages the arithmetics behind
    /// this.
    ///
    /// Addresses used in `symbols` or `debug_session` have already been rebased relative to that
    /// load address, so that the caller only has to deal with addresses relative to the actual
    /// start of the image.
    pub fn load_address(&self) -> u64 {
        // For non-PIC executables (e_type == ET_EXEC), the load address is
        // the start address of the first PT_LOAD segment.  (ELF requires
        // the segments to be sorted by load address.)  For PIC executables
        // and dynamic libraries (e_type == ET_DYN), this address will
        // normally be zero.
        for phdr in &self.elf.program_headers {
            if phdr.p_type == elf::program_header::PT_LOAD {
                return phdr.p_vaddr;
            }
        }

        0
    }

    /// Determines whether this object exposes a public symbol table.
    pub fn has_symbols(&self) -> bool {
        !self.elf.syms.is_empty()
    }

    /// Returns an iterator over symbols in the public symbol table.
    pub fn symbols(&self) -> ElfSymbolIterator<'data, '_> {
        ElfSymbolIterator {
            symbols: self.elf.syms.iter(),
            strtab: &self.elf.strtab,
            sections: &self.elf.section_headers,
            load_addr: self.load_address(),
        }
    }

    /// Returns an ordered map of symbols in the symbol table.
    pub fn symbol_map(&self) -> SymbolMap<'data> {
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
    /// ELF files generally use DWARF debugging information, which is also used by MachO containers
    /// on macOS.
    ///
    /// Constructing this session will also work if the object does not contain debugging
    /// information, in which case the session will be a no-op. This can be checked via
    /// [`has_debug_info`](struct.ElfObject.html#method.has_debug_info).
    pub fn debug_session(&self) -> Result<DwarfDebugSession<'data>, DwarfError> {
        let symbols = self.symbol_map();
        DwarfDebugSession::parse(self, symbols, self.load_address() as i64, self.kind())
    }

    /// Determines whether this object contains stack unwinding information.
    pub fn has_unwind_info(&self) -> bool {
        self.has_section("eh_frame") || self.has_section("debug_frame")
    }

    /// Determines whether this object contains embedded source.
    pub fn has_sources(&self) -> bool {
        false
    }

    /// Returns the raw data of the ELF file.
    pub fn data(&self) -> &'data [u8] {
        self.data
    }

    /// Decompresses the given compressed section data, if supported.
    fn decompress_section(&self, section_data: &[u8]) -> Option<Vec<u8>> {
        let (size, compressed) = if section_data.starts_with(b"ZLIB") {
            // The GNU compression header is a 4 byte magic "ZLIB", followed by an 8-byte big-endian
            // size prefix of the decompressed data. This adds up to 12 bytes of GNU header.
            if section_data.len() < 12 {
                return None;
            }

            let mut size_bytes = [0; 8];
            size_bytes.copy_from_slice(&section_data[4..12]);

            (u64::from_be_bytes(size_bytes), &section_data[12..])
        } else {
            let container = self.elf.header.container().ok()?;
            let endianness = self.elf.header.endianness().ok()?;
            let context = Ctx::new(container, endianness);

            let compression = CompressionHeader::parse(&section_data, 0, context).ok()?;
            if compression.ch_type != ELFCOMPRESS_ZLIB {
                return None;
            }

            let compressed = &section_data[CompressionHeader::size(context)..];
            (compression.ch_size, compressed)
        };

        let mut decompressed = Vec::with_capacity(size as usize);
        Decompress::new(true)
            .decompress_vec(compressed, &mut decompressed, FlushDecompress::Finish)
            .ok()?;

        Some(decompressed)
    }

    /// Locates and reads a section in an ELF binary.
    fn find_section(&self, name: &str) -> Option<(bool, DwarfSection<'data>)> {
        for header in &self.elf.section_headers {
            // NB: Symbolic does not support MIPS, but if it did we would also need to check
            // SHT_MIPS_DWARF sections.
            if header.sh_type != elf::section_header::SHT_PROGBITS {
                continue;
            }

            if let Some(Ok(section_name)) = self.elf.shdr_strtab.get(header.sh_name) {
                let offset = header.sh_offset as usize;
                if offset == 0 {
                    // We're defensive here. On darwin, dsymutil leaves phantom section headers
                    // while stripping their data from the file by setting their offset to 0. We
                    // know that no section can start at an absolute file offset of zero, so we can
                    // safely skip them in case similar things happen on linux.
                    return None;
                }

                if section_name.is_empty() {
                    continue;
                }

                // Before SHF_COMPRESSED was a thing, compressed sections were prefixed with `.z`.
                // Support this as an override to the flag.
                let (compressed, section_name) = match section_name.strip_prefix(".z") {
                    Some(name) => (true, name),
                    None => (header.sh_flags & SHF_COMPRESSED != 0, &section_name[1..]),
                };

                if section_name != name {
                    continue;
                }

                let size = header.sh_size as usize;
                let data = &self.data[offset..][..size];
                let section = DwarfSection {
                    data: Cow::Borrowed(data),
                    address: header.sh_addr,
                    offset: header.sh_offset,
                    align: header.sh_addralign,
                };

                return Some((compressed, section));
            }
        }

        None
    }

    /// Searches for a GNU build identifier node in an ELF file.
    ///
    /// Depending on the compiler and linker, the build ID can be declared in a
    /// PT_NOTE program header entry, the ".note.gnu.build-id" section, or even
    /// both.
    fn find_build_id(&self) -> Option<&'data [u8]> {
        // First, search the note program headers (PT_NOTE) for a NT_GNU_BUILD_ID.
        // We swallow all errors during this process and simply fall back to the
        // next method below.
        if let Some(mut notes) = self.elf.iter_note_headers(self.data) {
            while let Some(Ok(note)) = notes.next() {
                if note.n_type == elf::note::NT_GNU_BUILD_ID {
                    return Some(note.desc);
                }
            }
        }

        // Some old linkers or compilers might not output the above PT_NOTE headers.
        // In that case, search for a note section (SHT_NOTE). We are looking for a
        // note within the ".note.gnu.build-id" section. Again, swallow all errors
        // and fall through if reading the section is not possible.
        if let Some(mut notes) = self
            .elf
            .iter_note_sections(self.data, Some(".note.gnu.build-id"))
        {
            while let Some(Ok(note)) = notes.next() {
                if note.n_type == elf::note::NT_GNU_BUILD_ID {
                    return Some(note.desc);
                }
            }
        }

        None
    }

    /// Converts an ELF object identifier into a `DebugId`.
    ///
    /// The identifier data is first truncated or extended to match 16 byte size of
    /// Uuids. If the data is declared in little endian, the first three Uuid fields
    /// are flipped to match the big endian expected by the breakpad processor.
    ///
    /// The `DebugId::appendix` field is always `0` for ELF.
    fn compute_debug_id(&self, identifier: &[u8]) -> DebugId {
        // Make sure that we have exactly UUID_SIZE bytes available
        let mut data = [0; UUID_SIZE];
        let len = std::cmp::min(identifier.len(), UUID_SIZE);
        data[0..len].copy_from_slice(&identifier[0..len]);

        if self.elf.little_endian {
            // The file ELF file targets a little endian architecture. Convert to
            // network byte order (big endian) to match the Breakpad processor's
            // expectations. For big endian object files, this is not needed.
            data[0..4].reverse(); // uuid field 1
            data[4..6].reverse(); // uuid field 2
            data[6..8].reverse(); // uuid field 3
        }

        Uuid::from_slice(&data)
            .map(DebugId::from_uuid)
            .unwrap_or_default()
    }
}

impl fmt::Debug for ElfObject<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ElfObject")
            .field("code_id", &self.code_id())
            .field("debug_id", &self.debug_id())
            .field("arch", &self.arch())
            .field("kind", &self.kind())
            .field("load_address", &format_args!("{:#x}", self.load_address()))
            .field("has_symbols", &self.has_symbols())
            .field("has_debug_info", &self.has_debug_info())
            .field("has_unwind_info", &self.has_unwind_info())
            .finish()
    }
}

impl<'slf, 'data: 'slf> AsSelf<'slf> for ElfObject<'data> {
    type Ref = ElfObject<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        self
    }
}

impl<'data> Parse<'data> for ElfObject<'data> {
    type Error = ElfError;

    fn test(data: &[u8]) -> bool {
        Self::test(data)
    }

    fn parse(data: &'data [u8]) -> Result<Self, ElfError> {
        Self::parse(data)
    }
}

impl<'data: 'object, 'object> ObjectLike<'data, 'object> for ElfObject<'data> {
    type Error = DwarfError;
    type Session = DwarfDebugSession<'data>;
    type SymbolIterator = ElfSymbolIterator<'data, 'object>;

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
}

impl<'data> Dwarf<'data> for ElfObject<'data> {
    fn endianity(&self) -> Endian {
        if self.elf.little_endian {
            Endian::Little
        } else {
            Endian::Big
        }
    }

    fn raw_section(&self, name: &str) -> Option<DwarfSection<'data>> {
        let (_, section) = self.find_section(name)?;
        Some(section)
    }

    fn section(&self, name: &str) -> Option<DwarfSection<'data>> {
        let (compressed, mut section) = self.find_section(name)?;

        if compressed {
            let decompressed = self.decompress_section(&section.data)?;
            section.data = Cow::Owned(decompressed);
        }

        Some(section)
    }
}

/// An iterator over symbols in the ELF file.
///
/// Returned by [`ElfObject::symbols`](struct.ElfObject.html#method.symbols).
pub struct ElfSymbolIterator<'data, 'object> {
    symbols: elf::sym::SymIterator<'data>,
    strtab: &'object strtab::Strtab<'data>,
    sections: &'object [elf::SectionHeader],
    load_addr: u64,
}

impl<'data, 'object> Iterator for ElfSymbolIterator<'data, 'object> {
    type Item = Symbol<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(symbol) = self.symbols.next() {
            // Only check for function symbols.
            if symbol.st_type() != elf::sym::STT_FUNC {
                continue;
            }

            // Sanity check of the symbol address. Since we only intend to iterate over function
            // symbols, they need to be mapped after the image's load address.
            if symbol.st_value < self.load_addr {
                continue;
            }

            let section = match symbol.st_shndx {
                self::SHN_UNDEF => None,
                index => self.sections.get(index),
            };

            // We are only interested in symbols pointing into sections with executable flag.
            if !section.map_or(false, |header| header.is_executable()) {
                continue;
            }

            let name = self
                .strtab
                .get(symbol.st_name)
                .and_then(Result::ok)
                .map(Cow::Borrowed);

            return Some(Symbol {
                name,
                address: symbol.st_value - self.load_addr,
                size: symbol.st_size,
            });
        }

        None
    }
}
