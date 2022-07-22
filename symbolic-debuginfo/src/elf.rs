//! Support for the Executable and Linkable Format, used on Linux.

use std::borrow::Cow;
use std::convert::TryInto;
use std::error::Error;
use std::ffi::CStr;
use std::fmt;

use core::cmp;
use flate2::{Decompress, FlushDecompress};
use goblin::elf::compression_header::{CompressionHeader, ELFCOMPRESS_ZLIB};
use goblin::elf::SectionHeader;
use goblin::elf64::sym::SymIterator;
use goblin::strtab::Strtab;
use goblin::{
    container::{Container, Ctx},
    elf, strtab,
};
use scroll::Pread;
use thiserror::Error;

use symbolic_common::{Arch, AsSelf, CodeId, DebugId, Uuid};

use crate::base::*;
use crate::dwarf::{Dwarf, DwarfDebugSession, DwarfError, DwarfSection, Endian};
use crate::Parse;

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
    is_malformed: bool,
}

impl<'data> ElfObject<'data> {
    /// Tests whether the buffer could contain an ELF object.
    pub fn test(data: &[u8]) -> bool {
        data.get(0..elf::header::SELFMAG)
            .map_or(false, |data| data == elf::header::ELFMAG)
    }

    // Pulled from https://github.com/m4b/goblin/blob/master/src/elf/mod.rs#L393-L424 as it
    // currently isn't public, but we need this to parse an ELF.
    fn gnu_hash_len(bytes: &[u8], offset: usize, ctx: Ctx) -> goblin::error::Result<usize> {
        let buckets_num = bytes.pread_with::<u32>(offset, ctx.le)? as usize;
        let min_chain = bytes.pread_with::<u32>(offset + 4, ctx.le)? as usize;
        let bloom_size = bytes.pread_with::<u32>(offset + 8, ctx.le)? as usize;
        // We could handle min_chain==0 if we really had to, but it shouldn't happen.
        if buckets_num == 0 || min_chain == 0 || bloom_size == 0 {
            return Err(goblin::error::Error::Malformed(format!(
                "Invalid DT_GNU_HASH: buckets_num={} min_chain={} bloom_size={}",
                buckets_num, min_chain, bloom_size
            )));
        }
        // Find the last bucket.
        let buckets_offset = offset + 16 + bloom_size * if ctx.container.is_big() { 8 } else { 4 };
        let mut max_chain = 0;
        for bucket in 0..buckets_num {
            let chain = bytes.pread_with::<u32>(buckets_offset + bucket * 4, ctx.le)? as usize;
            if max_chain < chain {
                max_chain = chain;
            }
        }
        if max_chain < min_chain {
            return Ok(0);
        }
        // Find the last chain within the bucket.
        let mut chain_offset = buckets_offset + buckets_num * 4 + (max_chain - min_chain) * 4;
        loop {
            let hash = bytes.pread_with::<u32>(chain_offset, ctx.le)?;
            max_chain += 1;
            chain_offset += 4;
            if hash & 1 != 0 {
                return Ok(max_chain);
            }
        }
    }

    // Pulled from https://github.com/m4b/goblin/blob/master/src/elf/mod.rs#L426-L434 as it
    // currently isn't public, but we need this to parse an ELF.
    fn hash_len(
        bytes: &[u8],
        offset: usize,
        machine: u16,
        ctx: Ctx,
    ) -> goblin::error::Result<usize> {
        // Based on readelf code.
        let nchain = if (machine == elf::header::EM_FAKE_ALPHA || machine == elf::header::EM_S390)
            && ctx.container.is_big()
        {
            bytes.pread_with::<u64>(offset.saturating_add(4), ctx.le)? as usize
        } else {
            bytes.pread_with::<u32>(offset.saturating_add(4), ctx.le)? as usize
        };
        Ok(nchain)
    }

    /// Tries to parse an ELF object from the given slice. Will return a partially parsed ELF object
    /// if at least the program and section headers can be parsed.
    pub fn parse(data: &'data [u8]) -> Result<Self, ElfError> {
        let header =
            elf::Elf::parse_header(data).map_err(|_| ElfError::new("ELF header unreadable"))?;
        // dummy Elf with only header
        let mut obj =
            elf::Elf::lazy_parse(header).map_err(|_| ElfError::new("cannot parse ELF header"))?;

        let ctx = Ctx {
            container: if obj.is_64 {
                Container::Big
            } else {
                Container::Little
            },
            le: if obj.little_endian {
                scroll::Endian::Little
            } else {
                scroll::Endian::Big
            },
        };

        macro_rules! return_partial_on_err {
            ($parse_func:expr) => {
                if let Ok(expected) = $parse_func() {
                    expected
                } else {
                    // does this snapshot?
                    return Ok(ElfObject {
                        elf: obj,
                        data,
                        is_malformed: true,
                    });
                }
            };
        }

        obj.program_headers =
            elf::ProgramHeader::parse(data, header.e_phoff as usize, header.e_phnum as usize, ctx)
                .map_err(|_| ElfError::new("unable to parse program headers"))?;

        for ph in &obj.program_headers {
            if ph.p_type == elf::program_header::PT_INTERP && ph.p_filesz != 0 {
                let count = (ph.p_filesz - 1) as usize;
                let offset = ph.p_offset as usize;
                obj.interpreter = data
                    .pread_with::<&str>(offset, ::scroll::ctx::StrCtx::Length(count))
                    .ok();
            }
        }

        obj.section_headers =
            SectionHeader::parse(data, header.e_shoff as usize, header.e_shnum as usize, ctx)
                .map_err(|_| ElfError::new("unable to parse section headers"))?;

        let get_strtab = |section_headers: &[SectionHeader], section_idx: usize| {
            if section_idx >= section_headers.len() {
                // FIXME: warn! here
                Ok(Strtab::default())
            } else {
                let shdr = &section_headers[section_idx];
                shdr.check_size(data.len())?;
                Strtab::parse(data, shdr.sh_offset as usize, shdr.sh_size as usize, 0x0)
            }
        };

        let strtab_idx = header.e_shstrndx as usize;
        obj.shdr_strtab = return_partial_on_err!(|| get_strtab(&obj.section_headers, strtab_idx));

        obj.syms = elf::Symtab::default();
        obj.strtab = Strtab::default();
        for shdr in &obj.section_headers {
            if shdr.sh_type as u32 == elf::section_header::SHT_SYMTAB {
                let size = shdr.sh_entsize;
                let count = if size == 0 { 0 } else { shdr.sh_size / size };
                obj.syms = return_partial_on_err!(|| elf::Symtab::parse(
                    data,
                    shdr.sh_offset as usize,
                    count as usize,
                    ctx
                ));

                obj.strtab = return_partial_on_err!(|| get_strtab(
                    &obj.section_headers,
                    shdr.sh_link as usize
                ));
            }
        }

        obj.soname = None;
        obj.libraries = vec![];
        obj.dynsyms = elf::Symtab::default();
        obj.dynrelas = elf::RelocSection::default();
        obj.dynrels = elf::RelocSection::default();
        obj.pltrelocs = elf::RelocSection::default();
        obj.dynstrtab = Strtab::default();
        let dynamic =
            return_partial_on_err!(|| elf::Dynamic::parse(data, &obj.program_headers, ctx));
        if let Some(ref dynamic) = dynamic {
            let dyn_info = &dynamic.info;
            obj.dynstrtab = return_partial_on_err!(|| Strtab::parse(
                data,
                dyn_info.strtab,
                dyn_info.strsz,
                0x0
            ));

            if dyn_info.soname != 0 {
                // FIXME: warn! here
                obj.soname = obj.dynstrtab.get_at(dyn_info.soname);
            }
            if dyn_info.needed_count > 0 {
                obj.libraries = dynamic.get_libraries(&obj.dynstrtab);
            }
            // parse the dynamic relocations
            obj.dynrelas = return_partial_on_err!(|| elf::RelocSection::parse(
                data,
                dyn_info.rela,
                dyn_info.relasz,
                true,
                ctx
            ));
            obj.dynrels = return_partial_on_err!(|| elf::RelocSection::parse(
                data,
                dyn_info.rel,
                dyn_info.relsz,
                false,
                ctx
            ));
            let is_rela = dyn_info.pltrel as u64 == elf::dynamic::DT_RELA;
            obj.pltrelocs = return_partial_on_err!(|| elf::RelocSection::parse(
                data,
                dyn_info.jmprel,
                dyn_info.pltrelsz,
                is_rela,
                ctx
            ));

            let mut num_syms = if let Some(gnu_hash) = dyn_info.gnu_hash {
                return_partial_on_err!(|| ElfObject::gnu_hash_len(data, gnu_hash as usize, ctx))
            } else if let Some(hash) = dyn_info.hash {
                return_partial_on_err!(|| ElfObject::hash_len(
                    data,
                    hash as usize,
                    header.e_machine,
                    ctx
                ))
            } else {
                0
            };
            let max_reloc_sym = obj
                .dynrelas
                .iter()
                .chain(obj.dynrels.iter())
                .chain(obj.pltrelocs.iter())
                .fold(0, |num, reloc| cmp::max(num, reloc.r_sym));
            if max_reloc_sym != 0 {
                num_syms = cmp::max(num_syms, max_reloc_sym + 1);
            }

            obj.dynsyms =
                return_partial_on_err!(|| elf::Symtab::parse(data, dyn_info.symtab, num_syms, ctx));
        }

        obj.shdr_relocs = vec![];
        for (idx, section) in obj.section_headers.iter().enumerate() {
            let is_rela = section.sh_type == elf::section_header::SHT_RELA;
            if is_rela || section.sh_type == elf::section_header::SHT_REL {
                return_partial_on_err!(|| section.check_size(data.len()));
                let sh_relocs = return_partial_on_err!(|| elf::RelocSection::parse(
                    data,
                    section.sh_offset as usize,
                    section.sh_size as usize,
                    is_rela,
                    ctx,
                ));
                obj.shdr_relocs.push((idx, sh_relocs));
            }
        }

        obj.versym = return_partial_on_err!(|| elf::symver::VersymSection::parse(
            data,
            &obj.section_headers,
            ctx
        ));
        obj.verdef = return_partial_on_err!(|| elf::symver::VerdefSection::parse(
            data,
            &obj.section_headers,
            ctx
        ));
        obj.verneed = return_partial_on_err!(|| elf::symver::VerneedSection::parse(
            data,
            &obj.section_headers,
            ctx
        ));

        Ok(ElfObject {
            elf: obj,
            data,
            is_malformed: false,
        })
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
            .map(CodeId::from_binary)
    }

    /// The debug link of this object.
    ///
    /// The debug link is an alternative to the build id for specifying the location
    /// of an ELF's debugging information. It refers to a filename that can be used
    /// to build various debug paths where debuggers can look for the debug files.
    ///
    /// # Errors
    ///
    /// - None if there is no gnu_debuglink section
    /// - DebugLinkError if this section exists, but is malformed
    pub fn debug_link(&self) -> Result<Option<DebugLink>, DebugLinkError> {
        self.section("gnu_debuglink")
            .map(|section| DebugLink::from_data(section.data, self.endianity()))
            .transpose()
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
        !self.elf.syms.is_empty() || !self.elf.dynsyms.is_empty()
    }

    /// Returns an iterator over symbols in the public symbol table.
    pub fn symbols(&self) -> ElfSymbolIterator<'data, '_> {
        ElfSymbolIterator {
            symbols: self.elf.syms.iter(),
            strtab: &self.elf.strtab,
            dynamic_symbols: self.elf.dynsyms.iter(),
            dynamic_strtab: &self.elf.dynstrtab,
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

    /// Determines whether this object is malformed and was only partially parsed
    pub fn is_malformed(&self) -> bool {
        self.is_malformed
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

            let compression = CompressionHeader::parse(section_data, 0, context).ok()?;
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
            // The section type is usually SHT_PROGBITS, but some compilers also use
            // SHT_X86_64_UNWIND and SHT_MIPS_DWARF. We apply the same approach as elfutils,
            // matching against SHT_NOBITS, instead.
            if header.sh_type == elf::section_header::SHT_NOBITS {
                continue;
            }

            if let Some(section_name) = self.elf.shdr_strtab.get_at(header.sh_name) {
                let offset = header.sh_offset as usize;
                if offset == 0 {
                    // We're defensive here. On darwin, dsymutil leaves phantom section headers
                    // while stripping their data from the file by setting their offset to 0. We
                    // know that no section can start at an absolute file offset of zero, so we can
                    // safely skip them in case similar things happen on linux.
                    continue;
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
            .field("is_malformed", &self.is_malformed())
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

    fn is_malformed(&self) -> bool {
        self.is_malformed()
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
    dynamic_symbols: elf::sym::SymIterator<'data>,
    dynamic_strtab: &'object strtab::Strtab<'data>,
    sections: &'object [elf::SectionHeader],
    load_addr: u64,
}

impl<'data, 'object> Iterator for ElfSymbolIterator<'data, 'object> {
    type Item = Symbol<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        fn get_symbols<'data>(
            symbols: &mut SymIterator,
            strtab: &Strtab<'data>,
            load_addr: u64,
            sections: &[SectionHeader],
        ) -> Option<Symbol<'data>> {
            for symbol in symbols {
                // Only check for function symbols.
                if symbol.st_type() != elf::sym::STT_FUNC {
                    continue;
                }

                // Sanity check of the symbol address. Since we only intend to iterate over function
                // symbols, they need to be mapped after the image's load address.
                if symbol.st_value < load_addr {
                    continue;
                }

                let section = match symbol.st_shndx {
                    self::SHN_UNDEF => None,
                    index => sections.get(index),
                };

                // We are only interested in symbols pointing into sections with executable flag.
                if !section.map_or(false, |header| header.is_executable()) {
                    continue;
                }

                let name = strtab.get_at(symbol.st_name).map(Cow::Borrowed);

                return Some(Symbol {
                    name,
                    address: symbol.st_value - load_addr,
                    size: symbol.st_size,
                });
            }

            None
        }

        get_symbols(
            &mut self.symbols,
            self.strtab,
            self.load_addr,
            self.sections,
        )
        .or_else(|| {
            get_symbols(
                &mut self.dynamic_symbols,
                self.dynamic_strtab,
                self.load_addr,
                self.sections,
            )
        })
    }
}

/// Parsed debug link section.
#[derive(Debug)]
pub struct DebugLink<'data> {
    filename: Cow<'data, CStr>,
    crc: u32,
}

impl<'data> DebugLink<'data> {
    /// Attempts to parse a debug link section from its data.
    ///
    /// The expected format for the section is:
    ///
    /// - A filename, with any leading directory components removed, followed by a zero byte,
    /// - zero to three bytes of padding, as needed to reach the next four-byte boundary within the section, and
    /// - a four-byte CRC checksum, stored in the same endianness used for the executable file itself.
    /// (from <https://sourceware.org/gdb/current/onlinedocs/gdb/Separate-Debug-Files.html#index-_002egnu_005fdebuglink-sections>)
    ///
    /// # Errors
    ///
    /// If the section data is malformed, in particular:
    /// - No NUL byte delimiting the filename from the CRC
    /// - Not enough space for the CRC checksum
    pub fn from_data(
        data: Cow<'data, [u8]>,
        endianity: Endian,
    ) -> Result<Self, DebugLinkError<'data>> {
        match data {
            Cow::Owned(data) => {
                let (filename, crc) = Self::from_borrowed_data(&data, endianity)
                    .map(|(filename, crc)| (filename.to_owned(), crc))
                    .map_err(|kind| DebugLinkError {
                        kind,
                        data: Cow::Owned(data),
                    })?;
                Ok(Self {
                    filename: Cow::Owned(filename),
                    crc,
                })
            }
            Cow::Borrowed(data) => {
                let (filename, crc) =
                    Self::from_borrowed_data(data, endianity).map_err(|kind| DebugLinkError {
                        kind,
                        data: Cow::Borrowed(data),
                    })?;
                Ok(Self {
                    filename: Cow::Borrowed(filename),
                    crc,
                })
            }
        }
    }

    fn from_borrowed_data(
        data: &[u8],
        endianity: Endian,
    ) -> Result<(&CStr, u32), DebugLinkErrorKind> {
        let nul_pos = data
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(DebugLinkErrorKind::MissingNul)?;

        if nul_pos + 1 == data.len() {
            return Err(DebugLinkErrorKind::MissingCrc {
                filename_len_with_nul: nul_pos + 1,
            });
        }

        let filename = &data[..nul_pos + 1];

        // let's be liberal and assume that the padding is correct and all 0s,
        // and just check that we have enough remaining length for the CRC.
        let crc = data
            .get(nul_pos + 1..)
            .and_then(|crc| crc.get(crc.len() - 4..))
            .ok_or(DebugLinkErrorKind::MissingCrc {
                filename_len_with_nul: filename.len(),
            })?;

        let crc: [u8; 4] = crc.try_into().map_err(|_| DebugLinkErrorKind::MissingCrc {
            filename_len_with_nul: filename.len(),
        })?;

        let crc = match endianity {
            Endian::Little => u32::from_le_bytes(crc),
            Endian::Big => u32::from_be_bytes(crc),
        };

        let filename =
            CStr::from_bytes_with_nul(filename).map_err(|_| DebugLinkErrorKind::MissingNul)?;

        Ok((filename, crc))
    }

    /// The debug link filename
    pub fn filename(&self) -> &CStr {
        &self.filename
    }

    /// The CRC checksum associated with the debug link file
    pub fn crc(&self) -> u32 {
        self.crc
    }
}

/// Kind of errors that can occur while parsing a debug link section.
#[derive(Debug, Error)]
pub enum DebugLinkErrorKind {
    /// No NUL byte delimiting the filename from the CRC
    #[error("missing NUL character")]
    MissingNul,
    /// Not enough space in the section data for the CRC checksum
    #[error("missing CRC")]
    MissingCrc {
        /// Size of the filename part of the section including the NUL character
        filename_len_with_nul: usize,
    },
}

/// Errors that can occur while parsing a debug link section.
#[derive(Debug, Error)]
#[error("could not parse debug link section")]
pub struct DebugLinkError<'data> {
    #[source]
    /// The kind of error that occurred.
    pub kind: DebugLinkErrorKind,
    /// The original data of the debug section.
    pub data: Cow<'data, [u8]>,
}
