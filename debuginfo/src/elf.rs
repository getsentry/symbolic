use std::borrow::Cow;
use std::cmp;

use flate2::{Decompress, FlushDecompress};
use goblin::elf::compression_header::{CompressionHeader, ELFCOMPRESS_ZLIB};
use goblin::{container::Ctx, elf};
use uuid::Uuid;

use symbolic_common::types::DebugId;

const UUID_SIZE: usize = 16;
const PAGE_SIZE: usize = 4096;

/// A section inside an ELF binary.
pub struct ElfSection<'elf, 'data> {
    pub header: &'elf elf::SectionHeader,
    pub data: Cow<'data, [u8]>,
}

/// Decompresses the given compressed section data, if supported.
fn decompress_section(elf: &elf::Elf<'_>, data: &[u8]) -> Option<Vec<u8>> {
    let container = elf.header.container().ok()?;
    let endianness = elf.header.endianness().ok()?;
    let context = Ctx::new(container, endianness);

    let compression = CompressionHeader::parse(&data, 0, context).ok()?;
    if compression.ch_type != ELFCOMPRESS_ZLIB {
        return None;
    }

    let compressed = &data[CompressionHeader::size(&context)..];
    let mut decompressed = Vec::with_capacity(compression.ch_size as usize);
    Decompress::new(true)
        .decompress_vec(compressed, &mut decompressed, FlushDecompress::Finish)
        .ok()?;

    Some(decompressed)
}

/// Locates and reads a section in an ELF binary.
pub fn find_elf_section<'elf, 'data>(
    elf: &'elf elf::Elf<'_>,
    data: &'data [u8],
    sh_type: u32,
    name: &str,
) -> Option<ElfSection<'elf, 'data>> {
    for header in &elf.section_headers {
        if header.sh_type != sh_type {
            continue;
        }

        if let Some(Ok(section_name)) = elf.shdr_strtab.get(header.sh_name) {
            if section_name != name {
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
            let mut section_data = Cow::Borrowed(&data[offset..][..size]);

            // Check for zlib compression of the section data. Once we've arrived here, we can
            // return None on error since each section will only occur once.
            if header.sh_flags & u64::from(elf::section_header::SHF_COMPRESSED) != 0 {
                section_data = Cow::Owned(decompress_section(elf, &section_data)?);
            }

            return Some(ElfSection {
                header,
                data: section_data,
            });
        }
    }

    None
}

/// Checks whether an ELF binary contains a section.
///
/// This is useful to determine whether the binary contains certain information
/// without loading its section data.
pub fn has_elf_section(elf: &elf::Elf<'_>, sh_type: u32, name: &str) -> bool {
    for header in &elf.section_headers {
        if header.sh_type != sh_type {
            continue;
        }

        if let Some(Ok(section_name)) = elf.shdr_strtab.get(header.sh_name) {
            if section_name != name {
                continue;
            }

            return header.sh_offset != 0;
        }
    }

    false
}

/// Searches for a GNU build identifier node in an ELF file.
///
/// Depending on the compiler and linker, the build ID can be declared in a
/// PT_NOTE program header entry, the ".note.gnu.build-id" section, or even
/// both.
fn find_build_id<'data>(elf: &elf::Elf<'data>, data: &'data [u8]) -> Option<&'data [u8]> {
    // First, search the note program headers (PT_NOTE) for a NT_GNU_BUILD_ID.
    // We swallow all errors during this process and simply fall back to the
    // next method below.
    if let Some(mut notes) = elf.iter_note_headers(data) {
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
    if let Some(mut notes) = elf.iter_note_sections(data, Some(".note.gnu.build-id")) {
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
fn create_elf_id(identifier: &[u8], little_endian: bool) -> Option<DebugId> {
    // Make sure that we have exactly UUID_SIZE bytes available
    let mut data = [0 as u8; UUID_SIZE];
    let len = cmp::min(identifier.len(), UUID_SIZE);
    data[0..len].copy_from_slice(&identifier[0..len]);

    if little_endian {
        // The file ELF file targets a little endian architecture. Convert to
        // network byte order (big endian) to match the Breakpad processor's
        // expectations. For big endian object files, this is not needed.
        data[0..4].reverse(); // uuid field 1
        data[4..6].reverse(); // uuid field 2
        data[6..8].reverse(); // uuid field 3
    }

    Uuid::from_slice(&data).ok().map(DebugId::from_uuid)
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
pub fn get_elf_id(elf: &elf::Elf<'_>, data: &[u8]) -> Option<DebugId> {
    // Search for a GNU build identifier node in the program headers or the
    // build ID section. If errors occur during this process, fall through
    // silently to the next method.
    if let Some(identifier) = find_build_id(elf, data) {
        return create_elf_id(identifier, elf.little_endian);
    }

    // We were not able to locate the build ID, so fall back to hashing the
    // first page of the ".text" (program code) section. This algorithm XORs
    // 16-byte chunks directly into a UUID buffer.
    if let Some(section) = find_elf_section(elf, data, elf::section_header::SHT_PROGBITS, ".text") {
        let mut hash = [0; UUID_SIZE];
        for i in 0..cmp::min(section.data.len(), PAGE_SIZE) {
            hash[i % UUID_SIZE] ^= section.data[i];
        }

        return create_elf_id(&hash, elf.little_endian);
    }

    None
}

/// Gets the virtual memory address of this object's .text (code) section.
pub fn get_elf_vmaddr(elf: &elf::Elf<'_>) -> u64 {
    // For non-PIC executables (e_type == ET_EXEC), the load address is
    // the start address of the first PT_LOAD segment.  (ELF requires
    // the segments to be sorted by load address.)  For PIC executables
    // and dynamic libraries (e_type == ET_DYN), this address will
    // normally be zero.
    for phdr in &elf.program_headers {
        if phdr.p_type == elf::program_header::PT_LOAD && phdr.is_executable() {
            return phdr.p_vaddr;
        }
    }

    0
}
