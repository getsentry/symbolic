use std::borrow;
use std::collections::BTreeMap;

use object::{Object, ObjectSection};
use scroll::Pread;

mod binary;
mod metadata;
pub(crate) mod utils;

#[cfg(test)]
mod tests {
    use std::borrow;
    use std::fs::File;
    use std::path::PathBuf;

    use object::{BigEndian, Object, ObjectSection, ObjectSymbol};

    use crate::binary::{DwarfData, Il2CppCodeGenModule};
    use crate::metadata::Il2CppMetadata;

    use super::*;

    fn first_arch(buf: &[u8]) -> &[u8] {
        let arches = object::macho::FatHeader::parse_arch32(buf).unwrap();
        let arch = arches[0];
        let offset = arch.offset.get(BigEndian) as usize;
        let size = arch.size.get(BigEndian) as usize;
        &buf[offset..(offset + size)]
    }

    #[test]
    fn test_fn_name() {
        let fixtures_dir = PathBuf::from("../../sentry-unity-il2cpp-line-numbers/Builds");

        let metadata_path = fixtures_dir
            .join("IL2CPP.app/Contents/Resources/Data/il2cpp_data/Metadata/global-metadata.dat");
        let metadata_file = File::open(metadata_path).unwrap();
        let metadata_buf = unsafe { memmap2::Mmap::map(&metadata_file) }.unwrap();

        let metadata = Il2CppMetadata::parse(&metadata_buf).unwrap();
        dbg!(metadata);

        let dylib_path = fixtures_dir.join("IL2CPP.app/Contents/Frameworks/GameAssembly.dylib");
        let dylib_file = File::open(dylib_path).unwrap();
        let dylib_buf = unsafe { memmap2::Mmap::map(&dylib_file) }.unwrap();
        let dylib_arch_buf = first_arch(&dylib_buf);
        let dylib = object::File::parse(dylib_arch_buf).unwrap();

        let dsym_path = fixtures_dir.join("IL2CPP_BackUpThisFolder_ButDontShipItWithYourGame/GameAssembly.dSYM/Contents/Resources/DWARF/GameAssembly.dylib");
        let dsym_file = File::open(dsym_path).unwrap();
        let dsym_buf = unsafe { memmap2::Mmap::map(&dsym_file) }.unwrap();
        let dsym_arch_buf = first_arch(&dsym_buf);
        let dsym = object::File::parse(dsym_arch_buf).unwrap();

        let object = &dsym;

        let endian = if object.is_little_endian() {
            gimli::RunTimeEndian::Little
        } else {
            gimli::RunTimeEndian::Big
        };

        // Load a section and return as `Cow<[u8]>`.
        let load_section = |id: gimli::SectionId| -> Result<borrow::Cow<[u8]>, gimli::Error> {
            match object.section_by_name(id.name()) {
                Some(ref section) => Ok(section
                    .uncompressed_data()
                    .unwrap_or(borrow::Cow::Borrowed(&[][..]))),
                None => Ok(borrow::Cow::Borrowed(&[][..])),
            }
        };

        // Load all of the sections.
        let dwarf_cow = gimli::Dwarf::load(&load_section).unwrap();

        // Borrow a `Cow<[u8]>` to create an `EndianSlice`.
        let borrow_section: &dyn for<'a> Fn(
            &'a borrow::Cow<[u8]>,
        )
            -> gimli::EndianSlice<'a, gimli::RunTimeEndian> =
            &|section| gimli::EndianSlice::new(&*section, endian);

        // Create `EndianSlice`s for all of the sections.
        let dwarf = dwarf_cow.borrow(&borrow_section);

        let dwarf_data = DwarfData::parse(&dwarf).unwrap();

        let codegenmodules_offset = dwarf_data.codegenmodules_offset.unwrap() as usize;
        let assembly_in_modules =
            dylib_arch_buf.pread::<u64>(codegenmodules_offset).unwrap() as usize;

        let module = Il2CppCodeGenModule::parse(dylib_arch_buf, assembly_in_modules).unwrap();
        dbg!(&module);

        for fn_ptr in module.method_pointers {
            dbg!(fn_ptr, dwarf_data.functions.get(fn_ptr));
        }
    }
}
