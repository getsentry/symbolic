use std::borrow;
use std::collections::BTreeMap;

use object::{Object, ObjectSection};
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

#[derive(Debug)]
struct DwarfData {
    /// A map from function pointer (DW_AT_low_pc) to function name (DW_AT_name).
    functions: BTreeMap<u64, String>,
    /// The offset of `g_CodeGenModules` (DW_TAG_variable) in the corresponding executable file.
    codegenmodules_offset: Option<u64>,
}

impl DwarfData {
    pub fn parse<R>(dwarf: &gimli::Dwarf<R>) -> anyhow::Result<Self>
    where
        R: gimli::Reader + std::ops::Deref<Target = [u8]> + PartialEq,
    {
        let mut functions = BTreeMap::new();
        let mut codegenmodules_offset = None;

        // Iterate over the compilation units.
        let mut iter = dwarf.units();
        while let Some(header) = iter.next()? {
            let unit = dwarf.unit(header)?;

            // Iterate over the Debugging Information Entries (DIEs) in the unit.
            let mut _depth = 0;
            let mut entries = unit.entries();
            while let Some((delta_depth, entry)) = entries.next_dfs()? {
                _depth += delta_depth;
                // println!("<{}><{:x}> {}", depth, entry.offset().0, entry.tag());

                let mut name = None;
                let mut low_pc = None;
                let mut location = None;

                // Iterate over the attributes in the DIE.
                let mut attrs = entry.attrs();
                while let Some(attr) = attrs.next()? {
                    match attr.name() {
                        gimli::constants::DW_AT_name => {
                            let attr_name = dwarf.attr_string(&unit, attr.value())?;
                            // TODO: this allocates all the time because of lifetime issues:
                            name = Some(std::str::from_utf8(&attr_name)?.to_string());
                        }
                        gimli::constants::DW_AT_low_pc => {
                            if let gimli::read::AttributeValue::Addr(addr) = attr.value() {
                                low_pc = Some(addr);
                            }
                        }
                        gimli::constants::DW_AT_location => {
                            location = attr.exprloc_value();
                        }
                        _ => {}
                    }
                }

                if let Some(name) = name {
                    if name == "g_CodeGenModules" {
                        if let Some(expr) = location {
                            let mut eval = expr.evaluation(unit.encoding());
                            let mut result = eval.evaluate().unwrap();
                            while result != gimli::EvaluationResult::Complete {
                                match result {
                                    gimli::EvaluationResult::RequiresRelocatedAddress(addr) => {
                                        result = eval.resume_with_relocated_address(addr).unwrap();
                                    }

                                    _ => break, // TODO: implement more cases
                                };
                            }

                            if result == gimli::EvaluationResult::Complete {
                                for res in eval.as_result() {
                                    if let gimli::Location::Address { address } = res.location {
                                        codegenmodules_offset = Some(address);
                                    }
                                }
                            }
                        }
                    }
                    if let Some(low_pc) = low_pc {
                        if entry.tag() == gimli::constants::DW_TAG_subprogram {
                            functions.insert(low_pc, name);
                        }
                    }
                }
            }
        }

        Ok(Self {
            functions,
            codegenmodules_offset,
        })
    }
}

#[derive(Debug)]
pub struct Il2CppCodeGenModule<'d> {
    name: &'d str,
    method_pointers: &'d [u64], // TODO: this needs to be generic
}

impl<'d> Il2CppCodeGenModule<'d> {
    pub fn parse(buf: &'d [u8], mut offset: usize) -> anyhow::Result<Self> {
        let offset = &mut offset;

        let name_ptr = buf.gread::<u64>(offset)? as usize;
        let name =
            buf.pread_with::<&str>(name_ptr, scroll::ctx::StrCtx::Delimiter(scroll::ctx::NULL))?;

        let num_methods = buf.gread::<u64>(offset)? as usize;
        let methods_ptr = buf.gread::<u64>(offset)? as usize;

        // TODO: turn this into a safe iter instead
        let method_pointers = unsafe {
            let raw_buf = buf.as_ptr().add(methods_ptr);
            std::slice::from_raw_parts(raw_buf as *const u64, num_methods)
        };

        Ok(Self {
            name,
            method_pointers,
        })
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
        // dbg!(&dwarf_data);

        // Builds/IL2CPP_BackUpThisFolder_ButDontShipItWithYourGame/il2cppOutput/Il2CppCodeRegistration.cpp
        // IL2CPP_EXTERN_C const Il2CppCodeGenModule* g_CodeGenModules[];
        // let codegenmodules_addr = find_symbol_addr(&dsym, b"g_CodeGenModules")
        //     .unwrap()
        //     .unwrap();

        // Builds/IL2CPP_BackUpThisFolder_ButDontShipItWithYourGame/il2cppOutput/Assembly-CSharp_CodeGen.c
        // IL2CPP_EXTERN_C const Il2CppCodeGenModule g_AssemblyU2DCSharp_CodeGenModule;
        // let assembly_addr = find_symbol_addr(&dsym, b"g_AssemblyU2DCSharp_CodeGenModule")
        //     .unwrap()
        //     .unwrap();

        // dbg!(codegenmodules_addr, assembly_addr);
        // 7210096
        // 7226480
        // => 16384 (the fatmach offset?)

        let codegenmodules_offset = dwarf_data.codegenmodules_offset.unwrap() as usize;
        let assembly_in_modules =
            dylib_arch_buf.pread::<u64>(codegenmodules_offset).unwrap() as usize;
        // assert_eq!(assembly_addr, assembly_in_modules);

        let module = Il2CppCodeGenModule::parse(dylib_arch_buf, assembly_in_modules).unwrap();
        dbg!(&module);

        for fn_ptr in module.method_pointers {
            dbg!(fn_ptr, dwarf_data.functions.get(fn_ptr));
        }
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

                    if name.starts_with(b"NewBehaviourScript") {
                        println!("{:?}:", std::str::from_utf8(&name));
                        println!("<{}><{:x}> {}", depth, entry.offset().0, entry.tag());

                        // Iterate over the attributes in the DIE.
                        let mut attrs = entry.attrs();
                        while let Some(attr) = attrs.next()? {
                            println!("   {}: {:?}", attr.name(), attr.value());
                        }
                    }

                    if name.as_ref() == symbol {
                        // parse the location
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
