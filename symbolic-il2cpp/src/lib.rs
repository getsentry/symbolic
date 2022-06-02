//! Experimental IL2CPP code.
//!
//! This create **is not supported**, it may break its API, it may completely disappear
//! again.  Do not consider this part of symbolic releases.  It is experimental code to
//! explore Unity IL2CPP debugging.
use std::borrow;
use std::collections::HashMap;

use binary::{build_native_method_map, DwarfData};
use metadata::Il2CppMetadata;
use object::{Object, ObjectSection};

mod binary;
mod line_mapping;
mod metadata;
pub mod usym;
pub mod usymlite;
pub(crate) mod utils;

pub use line_mapping::{LineMapping, ObjectLineMapping};

pub fn build_function_map(
    binary_buf: &[u8],
    dif_buf: &[u8],
    metadata_buf: &[u8],
) -> anyhow::Result<HashMap<String, String>> {
    // only handling dwarf for now:
    let dwarf_data = {
        let object = object::File::parse(dif_buf)?;

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
        let dwarf_cow = gimli::Dwarf::load(&load_section)?;

        // Borrow a `Cow<[u8]>` to create an `EndianSlice`.
        let borrow_section: &dyn for<'a> Fn(
            &'a borrow::Cow<[u8]>,
        )
            -> gimli::EndianSlice<'a, gimli::RunTimeEndian> =
            &|section| gimli::EndianSlice::new(&*section, endian);

        // Create `EndianSlice`s for all of the sections.
        let dwarf = dwarf_cow.borrow(&borrow_section);

        DwarfData::parse(&dwarf)?
    };

    // build the binary functions mapping:
    let native_map = build_native_method_map(binary_buf, dwarf_data)?;

    let metadata = Il2CppMetadata::parse(metadata_buf)?;
    let metadata_map = metadata.build_method_map()?;

    // dbg!(metadata_map.get("Assembly-CSharp.dll"), &native_map);

    // correlate the two maps:
    let mut mapping = HashMap::new();
    for (assembly_name, il_methods) in metadata_map {
        let native_methods = match native_map.get(&assembly_name) {
            Some(nm) => nm,
            None => continue,
        };
        for (idx, il_name) in il_methods {
            if let Some(native_name) = native_methods.get(&(idx as usize)) {
                mapping.insert(native_name.to_owned(), il_name);
            }
        }
    }

    Ok(mapping)
}

#[cfg(test)]
mod tests {
    use std::borrow;
    use std::fs::File;
    use std::path::PathBuf;

    use object::{BigEndian, Object, ObjectSection};
    use scroll::Pread;

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
    #[ignore]
    fn test_mapping_creation() {
        let fixtures_dir = PathBuf::from("../../sentry-unity-il2cpp-line-numbers/Builds/macOS");

        let metadata_path = fixtures_dir
            .join("IL2CPP.app/Contents/Resources/Data/il2cpp_data/Metadata/global-metadata.dat");
        let metadata_file = File::open(metadata_path).unwrap();
        let metadata_buf = unsafe { memmap2::Mmap::map(&metadata_file) }.unwrap();

        let dylib_path = fixtures_dir.join("IL2CPP.app/Contents/Frameworks/GameAssembly.dylib");
        let dylib_file = File::open(dylib_path).unwrap();
        let dylib_buf = unsafe { memmap2::Mmap::map(&dylib_file) }.unwrap();
        let dylib_arch_buf = first_arch(&dylib_buf);

        let dsym_path = fixtures_dir.join("IL2CPP_BackUpThisFolder_ButDontShipItWithYourGame/GameAssembly.dSYM/Contents/Resources/DWARF/GameAssembly.dylib");
        let dsym_file = File::open(dsym_path).unwrap();
        let dsym_buf = unsafe { memmap2::Mmap::map(&dsym_file) }.unwrap();
        let dsym_arch_buf = first_arch(&dsym_buf);

        let mapping = build_function_map(dylib_arch_buf, dsym_arch_buf, &metadata_buf).unwrap();
        dbg!(mapping);
    }

    #[test]
    #[ignore]
    fn test_metadata() {
        let fixtures_dir = PathBuf::from("../../sentry-unity-il2cpp-line-numbers/Builds/macOS");

        let metadata_path = fixtures_dir
            .join("IL2CPP.app/Contents/Resources/Data/il2cpp_data/Metadata/global-metadata.dat");
        let metadata_file = File::open(metadata_path).unwrap();
        let metadata_buf = unsafe { memmap2::Mmap::map(&metadata_file) }.unwrap();

        let metadata = Il2CppMetadata::parse(&metadata_buf).unwrap();
        let _ = dbg!(metadata.build_method_map());
    }

    #[test]
    #[ignore]
    fn test_binary() {
        let fixtures_dir = PathBuf::from("../../sentry-unity-il2cpp-line-numbers/Builds/macOS");

        let dylib_path = fixtures_dir.join("IL2CPP.app/Contents/Frameworks/GameAssembly.dylib");
        let dylib_file = File::open(dylib_path).unwrap();
        let dylib_buf = unsafe { memmap2::Mmap::map(&dylib_file) }.unwrap();
        let dylib_arch_buf = first_arch(&dylib_buf);

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

        let codegenmodules_offset = dwarf_data.code_registration_offset.unwrap() as usize;
        let assembly_in_modules =
            dylib_arch_buf.pread::<u64>(codegenmodules_offset).unwrap() as usize;

        let module = Il2CppCodeGenModule::parse(dylib_arch_buf, assembly_in_modules);
        dbg!(&module);

        // for fn_ptr in module.method_pointers {
        //     dbg!(fn_ptr, dwarf_data.functions.get(fn_ptr));
        // }
    }

    #[test]
    #[ignore]
    fn test_line_mapping() {
        let fixtures_dir = PathBuf::from("../../sentry-unity-il2cpp-line-numbers/Builds/macOS");

        let json_path = fixtures_dir.join("IL2CPP_BackUpThisFolder_ButDontShipItWithYourGame/il2cppOutput/Symbols/LineNumberMappings.json");
        let json_file = File::open(json_path).unwrap();
        let json_buf = unsafe { memmap2::Mmap::map(&json_file) }.unwrap();

        let line_mapping = LineMapping::parse(&json_buf).unwrap();
        let cpp_file_name ="/Users/bitfox/_Workspace/IL2CPP/Library/Bee/artifacts/MacStandalonePlayerBuildProgram/il2cppOutput/cpp/Assembly-CSharp.cpp";
        let cs_file_name = "/Users/bitfox/_Workspace/IL2CPP/Assets/NewBehaviourScript.cs";
        assert_eq!(line_mapping.lookup(cpp_file_name, 7), None);
        assert_eq!(
            line_mapping.lookup(cpp_file_name, 149),
            Some((cs_file_name, 10))
        );
        assert_eq!(
            line_mapping.lookup(cpp_file_name, 177),
            Some((cs_file_name, 17))
        );
    }
}
