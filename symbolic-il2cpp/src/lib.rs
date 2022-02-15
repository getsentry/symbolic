use scroll::Pread;

const IL2CPP_METADATA_MAGIC: u32 = 0xFAB11BAF; // TODO: use from_bytes_be

#[derive(Debug)]
pub struct Il2CppMetadata {}

impl Il2CppMetadata {
    pub fn parse(buf: &[u8]) -> anyhow::Result<Self> {
        let offset = &mut 0;

        let magic: u32 = buf.gread(offset)?;
        if magic != IL2CPP_METADATA_MAGIC {
            anyhow::bail!("wrong file magic");
        }

        let version: u32 = buf.gread(offset)?;
        if version != 29 {
            anyhow::bail!("wrong version: expected 29, got {}", version);
        }

        Ok(Self {})
    }
}

#[cfg(test)]
mod tests {
    use std::borrow;
    use std::fs::File;
    use std::path::PathBuf;

    use object::{BigEndian, Object, ObjectSection, ObjectSymbol};

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
        let dylib = object::File::parse(first_arch(&dylib_buf)).unwrap();

        let dsym_path = fixtures_dir.join("IL2CPP_BackUpThisFolder_ButDontShipItWithYourGame/GameAssembly.dSYM/Contents/Resources/DWARF/GameAssembly.dylib");
        let dsym_file = File::open(dsym_path).unwrap();
        let dsym_buf = unsafe { memmap2::Mmap::map(&dsym_file) }.unwrap();
        let dsym = object::File::parse(first_arch(&dsym_buf)).unwrap();

        // Builds/IL2CPP_BackUpThisFolder_ButDontShipItWithYourGame/il2cppOutput/Il2CppCodeRegistration.cpp
        // IL2CPP_EXTERN_C const Il2CppCodeGenModule* g_CodeGenModules[];
        let codegenmodules_addr = find_symbol_addr(&dsym, b"g_CodeGenModules")
            .unwrap()
            .unwrap();

        // Builds/IL2CPP_BackUpThisFolder_ButDontShipItWithYourGame/il2cppOutput/Assembly-CSharp_CodeGen.c
        // IL2CPP_EXTERN_C const Il2CppCodeGenModule g_AssemblyU2DCSharp_CodeGenModule;
        let assembly_addr = find_symbol_addr(&dsym, b"g_AssemblyU2DCSharp_CodeGenModule");

        dbg!(codegenmodules_addr, assembly_addr);
    }

    /// Finds the binary offset of the `g_CodeGenModules` symbol
    fn find_symbol_addr(object: &object::File, symbol: &[u8]) -> Result<Option<u64>, gimli::Error> {
        // TODO: would be nice to find this in the symbol table, but since its a private symbol,
        // we gotta look at the DWARF to find it.

        // for sym in dsym.symbols() {
        //     if sym.name() == Ok("g_CodeGenModules") {
        //         dbg!(sym);
        //     }
        // }

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

        // Iterate over the compilation units.
        let mut iter = dwarf.units();
        while let Some(header) = iter.next()? {
            // println!(
            //     "Unit at <.debug_info+0x{:x}>",
            //     header.offset().as_debug_info_offset().unwrap().0
            // );
            let unit = dwarf.unit(header)?;

            // Iterate over the Debugging Information Entries (DIEs) in the unit.
            let mut depth = 0;
            let mut entries = unit.entries();
            while let Some((delta_depth, entry)) = entries.next_dfs()? {
                depth += delta_depth;
                // println!("<{}><{:x}> {}", depth, entry.offset().0, entry.tag());

                // Iterate over the attributes in the DIE.
                // let mut attrs = entry.attrs();
                // while let Some(attr) = attrs.next()? {
                //     println!("   {}: {:?}", attr.name(), attr.value());
                // }

                if let Some(attr) = entry.attr_value(gimli::constants::DW_AT_name)? {
                    let name = dwarf.attr_string(&unit, attr)?;
                    if name.as_ref() == symbol {
                        if let Some(gimli::AttributeValue::Exprloc(expr)) =
                            entry.attr_value(gimli::constants::DW_AT_location)?
                        {
                            let mut eval = expr.evaluation(unit.encoding());
                            let mut result = eval.evaluate().unwrap();
                            while result != gimli::EvaluationResult::Complete {
                                match result {
                                    gimli::EvaluationResult::RequiresRelocatedAddress(addr) => {
                                        result = eval.resume_with_relocated_address(addr).unwrap();
                                    }

                                    _ => unimplemented!(),
                                };
                            }

                            let result = eval.result();
                            if let gimli::Location::Address { address } = result[0].location {
                                return Ok(Some(address));
                            };
                        }
                    }
                }
            }
        }
        Ok(None)
    }
}
