use std::borrow::Cow;
use std::fmt;
use std::io::Cursor;

use failure::Fail;
use flate2::{Decompress, FlushDecompress};
use goblin::elf::compression_header::{CompressionHeader, ELFCOMPRESS_ZLIB};
use goblin::{container::Ctx, elf, error::Error as GoblinError, strtab};

use symbolic_common::{Arch, AsSelf, DebugId, Uuid};

use crate::base::*;
use crate::dwarf::{Dwarf, DwarfDebugSession, DwarfError, Endian};
use crate::private::{HexFmt, Parse};

const UUID_SIZE: usize = 16;
const PAGE_SIZE: usize = 4096;
const SHN_UNDEF: usize = elf::section_header::SHN_UNDEF as usize;

#[derive(Debug, Fail)]
pub enum ElfError {
    #[fail(display = "invalid ELF file")]
    Goblin(#[fail(cause)] GoblinError),
}

pub struct ElfObject<'d> {
    elf: elf::Elf<'d>,
    data: &'d [u8],
}

impl<'d> ElfObject<'d> {
    pub fn test(data: &[u8]) -> bool {
        match goblin::peek(&mut Cursor::new(data)) {
            Ok(goblin::Hint::Elf(_)) => true,
            _ => false,
        }
    }

    pub fn parse(data: &'d [u8]) -> Result<Self, ElfError> {
        elf::Elf::parse(data)
            .map(|elf| ElfObject { elf, data })
            .map_err(ElfError::Goblin)
    }

    pub fn file_format(&self) -> FileFormat {
        FileFormat::Elf
    }

    /// Tries to obtain the object identifier of an ELF object.
    ///
    /// As opposed to Mach-O, ELF does not specify a unique ID for object files in
    /// its header. Compilers and linkers usually add either `SHT_NOTE` sections or
    /// `PT_NOTE` program header elements for this purpose.
    ///
    /// If neither of the above are present, this function will hash the first page
    /// of the `.text` section (program code) to synthesize a unique ID. This is
    /// likely not a valid UUID since was generated off a hash value.
    ///
    /// If all of the above fails, the identifier will be `None`.
    pub fn id(&self) -> DebugId {
        // Search for a GNU build identifier node in the program headers or the
        // build ID section. If errors occur during this process, fall through
        // silently to the next method.
        if let Some(identifier) = self.find_build_id() {
            return self.compute_debug_id(identifier);
        }

        // We were not able to locate the build ID, so fall back to hashing the
        // first page of the ".text" (program code) section. This algorithm XORs
        // 16-byte chunks directly into a UUID buffer.
        if let Some((_, data)) = self.find_section("text") {
            let mut hash = [0; UUID_SIZE];
            for i in 0..std::cmp::min(data.len(), PAGE_SIZE) {
                hash[i % UUID_SIZE] ^= data[i];
            }

            return self.compute_debug_id(&hash);
        }

        DebugId::default()
    }

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
            _ => Arch::Unknown,
        }
    }

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
            ObjectKind::Debug
        } else {
            kind
        }
    }

    pub fn load_address(&self) -> u64 {
        // For non-PIC executables (e_type == ET_EXEC), the load address is
        // the start address of the first PT_LOAD segment.  (ELF requires
        // the segments to be sorted by load address.)  For PIC executables
        // and dynamic libraries (e_type == ET_DYN), this address will
        // normally be zero.
        for phdr in &self.elf.program_headers {
            if phdr.p_type == elf::program_header::PT_LOAD && phdr.is_executable() {
                return phdr.p_vaddr;
            }
        }

        0
    }

    pub fn has_symbols(&self) -> bool {
        self.elf.syms.len() > 0
    }

    pub fn symbols(&self) -> ElfSymbolIterator<'d, '_> {
        ElfSymbolIterator {
            symbols: self.elf.syms.iter(),
            strtab: &self.elf.strtab,
            sections: &self.elf.section_headers,
            load_addr: self.load_address(),
        }
    }

    pub fn symbol_map(&self) -> SymbolMap<'d> {
        self.symbols().collect()
    }

    pub fn has_debug_info(&self) -> bool {
        self.has_section("debug_info")
    }

    pub fn debug_session(&self) -> Result<DwarfDebugSession<'d>, DwarfError> {
        let symbols = self.symbol_map();
        DwarfDebugSession::parse(self, symbols, self.load_address())
    }

    pub fn has_unwind_info(&self) -> bool {
        self.has_section("eh_frame") || self.has_section("debug_frame")
    }

    pub fn data(&self) -> &'d [u8] {
        self.data
    }

    /// Decompresses the given compressed section data, if supported.
    fn decompress_section(&self, section_data: &[u8]) -> Option<Vec<u8>> {
        let container = self.elf.header.container().ok()?;
        let endianness = self.elf.header.endianness().ok()?;
        let context = Ctx::new(container, endianness);

        let compression = CompressionHeader::parse(&section_data, 0, context).ok()?;
        if compression.ch_type != ELFCOMPRESS_ZLIB {
            return None;
        }

        let compressed = &section_data[CompressionHeader::size(&context)..];
        let mut decompressed = Vec::with_capacity(compression.ch_size as usize);
        Decompress::new(true)
            .decompress_vec(compressed, &mut decompressed, FlushDecompress::Finish)
            .ok()?;

        Some(decompressed)
    }

    /// Locates and reads a section in an ELF binary.
    fn find_section(&self, name: &str) -> Option<(&elf::SectionHeader, &'d [u8])> {
        for header in &self.elf.section_headers {
            if header.sh_type != elf::section_header::SHT_PROGBITS {
                continue;
            }

            if let Some(Ok(section_name)) = self.elf.shdr_strtab.get(header.sh_name) {
                if section_name.is_empty() || &section_name[1..] != name {
                    continue;
                }

                let offset = header.sh_offset as usize;
                if offset == 0 {
                    // We're defensive here. On darwin, dsymutil leaves phantom section headers while
                    // stripping their data from the file by setting their offset to 0. We know that no
                    // section can start at an absolute file offset of zero, so we can safely skip them
                    // in case similar things happen on linux.
                    return None;
                }

                let size = header.sh_size as usize;
                let data = &self.data[offset..][..size];
                return Some((header, data));
            }
        }

        None
    }

    /// Searches for a GNU build identifier node in an ELF file.
    ///
    /// Depending on the compiler and linker, the build ID can be declared in a
    /// PT_NOTE program header entry, the ".note.gnu.build-id" section, or even
    /// both.
    fn find_build_id(&self) -> Option<&'d [u8]> {
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
        let mut data = [0 as u8; UUID_SIZE];
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("ElfObject")
            .field("id", &self.id())
            .field("arch", &self.arch())
            .field("kind", &self.kind())
            .field("load_address", &HexFmt(self.load_address()))
            .field("has_symbols", &self.has_symbols())
            .field("has_debug_info", &self.has_debug_info())
            .field("has_unwind_info", &self.has_unwind_info())
            .finish()
    }
}

impl<'d, 'slf: 'd> AsSelf<'slf> for ElfObject<'d> {
    type Ref = ElfObject<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        self
    }
}

impl<'d> Parse<'d> for ElfObject<'d> {
    type Error = ElfError;

    fn test(data: &[u8]) -> bool {
        Self::test(data)
    }

    fn parse(data: &'d [u8]) -> Result<Self, ElfError> {
        Self::parse(data)
    }
}

impl<'d> ObjectLike for ElfObject<'d> {
    type Error = DwarfError;
    type Session = DwarfDebugSession<'d>;

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

impl<'d> Dwarf<'d> for ElfObject<'d> {
    fn endianity(&self) -> Endian {
        if self.elf.little_endian {
            Endian::Little
        } else {
            Endian::Big
        }
    }

    fn raw_data(&self, section: &str) -> Option<(u64, &'d [u8])> {
        let (header, data) = self.find_section(section)?;
        Some((header.sh_offset, data))
    }

    fn section_data(&self, section: &str) -> Option<(u64, Cow<'d, [u8]>)> {
        let (header, data) = self.find_section(section)?;

        // Check for zlib compression of the section data. Once we've arrived here, we can
        // return None on error since each section will only occur once.
        if header.sh_flags & u64::from(elf::section_header::SHF_COMPRESSED) != 0 {
            Some((header.sh_offset, Cow::Owned(self.decompress_section(data)?)))
        } else {
            Some((header.sh_offset, Cow::Borrowed(data)))
        }
    }
}

pub struct ElfSymbolIterator<'d, 'o> {
    symbols: elf::sym::SymIterator<'d>,
    strtab: &'o strtab::Strtab<'d>,
    sections: &'o [elf::SectionHeader],
    load_addr: u64,
}

impl<'d, 'o> Iterator for ElfSymbolIterator<'d, 'o> {
    type Item = Symbol<'d>;

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

            // We are only interested in symbols pointing into a program code section
            // (`SHT_PROGBITS`). Since the program might load R/W or R/O data sections via
            // SHT_PROGBITS, also check for the executable flag.
            let is_valid_section = section.map_or(false, |header| {
                header.sh_type == elf::section_header::SHT_PROGBITS && header.is_executable()
            });

            if !is_valid_section {
                continue;
            }

            let mut name = self
                .strtab
                .get(symbol.st_name)
                .and_then(Result::ok)
                .map(Cow::Borrowed);

            // Trim leading underscores from mangled C++ names.
            if let Some(Cow::Borrowed(ref mut name)) = name {
                if name.starts_with('_') {
                    *name = &name[1..];
                }
            }

            return Some(Symbol {
                name,
                address: symbol.st_value - self.load_addr,
                size: symbol.st_size,
            });
        }

        None
    }
}
