//! Contains utilities for parsing a WASM module to retrieve the information needed by [`super::WasmObject`]

use super::{WasmError, WasmObject};
use crate::base::{ObjectKind, Symbol};
use wasmparser::{ImportSectionEntryType, Payload, Validator};

pub(crate) fn parse<'data>(data: &'data [u8]) -> Result<WasmObject<'data>, WasmError> {
    let mut code_offset = 0;
    let mut build_id = None;
    let mut dwarf_sections = Vec::new();
    let mut kind = ObjectKind::Debug;

    // In "normal" wasm modules the only types will be function signatures, but in the future it
    // could contain types used for module linking, but we don't actually care about the types,
    // just that the function references a valid signature, so we just keep a bitset of the function
    // signatures to verify that
    let mut func_sigs = bitvec::vec::BitVec::<bitvec::order::Lsb0, usize>::new();
    let mut validator = Validator::new();
    let mut funcs = Vec::new();
    let mut num_imported_funcs = 0u32;
    let mut body_index = 0;

    // Parse the wasm file to pull out the function and their starting address, size, and name
    // Note that the order of the payloads here are the order that they will appear in (valid)
    // wasm binaries, other than the sections that we need to parse to validate the module, which
    // are at the end
    for payload in wasmparser::Parser::new(0).parse_all(data).flatten() {
        match payload {
            // This should always be first, and is necessary to prepare the validator since the
            // version determines which parts of the spec can be used
            Payload::Version { num, range } => {
                validator.version(num, &range)?;
            }
            // The type section contains, well, types, specifically, function signatures that are
            // later referenced by the function section.
            Payload::TypeSection(tsr) => {
                validator.type_section(&tsr)?;
                func_sigs.resize(tsr.get_count() as usize, false);
                let fs = func_sigs.as_mut_bitslice();

                for (i, ty) in tsr.into_iter().enumerate() {
                    if let wasmparser::TypeDef::Func(_) = ty? {
                        fs.set(i, true);
                    }
                }
            }
            // Imported functions and local functions both use the same ID space, but imported
            // functions are never exposed, so we just need to account for the id offset later
            // when parsing the local functions
            Payload::ImportSection(isr) => {
                validator.import_section(&isr)?;

                for import in isr {
                    let import = import?;
                    if let ImportSectionEntryType::Function(id) = import.ty {
                        if !func_sigs
                            .as_bitslice()
                            .get(id as usize)
                            .as_deref()
                            .unwrap_or(&false)
                        {
                            return Err(WasmError::UnknownFunctionType);
                        }

                        num_imported_funcs += 1;
                    }
                }
            }
            // The function section declares all of the local functions present in the module
            Payload::FunctionSection(fsr) => {
                validator.function_section(&fsr)?;

                if fsr.get_count() > 0 {
                    kind = ObjectKind::Library;
                }

                funcs.resize(
                    fsr.get_count() as usize,
                    Symbol {
                        name: None,
                        address: 0,
                        size: 0,
                    },
                );

                // We actually don't care about the type signature of the function, other than that
                // they exist
                for id in fsr {
                    if !func_sigs
                        .as_bitslice()
                        .get(id? as usize)
                        .as_deref()
                        .unwrap_or(&false)
                    {
                        return Err(WasmError::UnknownFunctionType);
                    }
                }
            }
            // The code section contains the actual function bodies, this payload is emitted at
            // the beginning of the section. This one is important as the code section offset is
            // used to calculate relative addresses in a `DwarfDebugSession`
            Payload::CodeSectionStart { range, count, .. } => {
                code_offset = range.start as u64;
                validator.code_section_start(count, &range)?;
            }
            // We get one of these for each local function body
            Payload::CodeSectionEntry(body) => {
                let validator = validator.code_section_entry()?;

                let (address, size) = get_function_info(body, validator)?;

                funcs[body_index].address = address;
                funcs[body_index].size = size;

                // Though we have an accurate? size of the function body, the old method of symbol
                // iterating with walrus extends the size of each body to be contiguous with the
                // next function, so we do the same, other than the final function
                if body_index > 0 {
                    funcs[body_index - 1].size = address - funcs[body_index - 1].address;
                }

                body_index += 1;
            }
            Payload::ModuleSectionStart { count, range, .. } => {
                validator.module_section_start(count, &range)?;
            }
            Payload::DataSection(dsr) => {
                validator.data_section(&dsr)?;
            }
            // There are several custom sections that we need
            Payload::CustomSection {
                name,
                data,
                data_offset,
                ..
            } => {
                match name {
                    // this section is not defined yet
                    // see https://github.com/WebAssembly/tool-conventions/issues/133
                    "build_id" => {
                        build_id = Some(data);
                    }
                    // All of the dwarf debug sections (.debug_frame, .debug_info etc) start with a `.`, and
                    // are the only ones we need for walking the debug info
                    debug if debug.starts_with('.') => {
                        dwarf_sections.push((name, data));
                    }
                    // The name section contains the symbol names for items, notably functions
                    "name" => {
                        let nsr = wasmparser::NameSectionReader::new(data, data_offset)?;

                        for name in nsr {
                            if let wasmparser::Name::Function(fnames) = name? {
                                let mut map = fnames.get_map()?;
                                for _ in 0..map.get_count() {
                                    let fname = map.read()?;

                                    // The names for imported functions are also in this table, but
                                    // we don't care about them
                                    if fname.index >= num_imported_funcs {
                                        if let Some(func) = funcs
                                            .get_mut((fname.index - num_imported_funcs) as usize)
                                        {
                                            func.name =
                                                dbg!(Some(std::borrow::Cow::Borrowed(fname.name)));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            // Final
            Payload::End => validator.end()?,

            // The following sections are not used by this crate, but some (eg table/memory/global)
            // are needed to validate the sections that we do care about, so we just validate all
            // of the sections we don't use to be sure
            Payload::TableSection(tsr) => {
                validator.table_section(&tsr)?;
            }
            Payload::MemorySection(msr) => {
                validator.memory_section(&msr)?;
            }
            Payload::TagSection(tsr) => {
                validator.tag_section(&tsr)?;
            }
            Payload::GlobalSection(gsr) => {
                validator.global_section(&gsr)?;
            }
            Payload::ExportSection(esr) => {
                validator.export_section(&esr)?;
            }
            Payload::StartSection { func, range } => {
                validator.start_section(func, &range)?;
            }
            Payload::ElementSection(esr) => {
                validator.element_section(&esr)?;
            }
            Payload::DataCountSection { count, range } => {
                validator.data_count_section(count, &range)?;
            }
            Payload::UnknownSection { id, range, .. } => {
                validator.unknown_section(id, &range)?;
            }
            _ => {}
        }
    }

    Ok(WasmObject {
        dwarf_sections,
        funcs,
        build_id,
        data,
        code_offset,
        kind,
    })
}

fn get_function_info(
    body: wasmparser::FunctionBody,
    mut validator: wasmparser::FuncValidator<wasmparser::ValidatorResources>,
) -> Result<(u64, u64), WasmError> {
    let mut body = body.get_binary_reader();

    // locals, we _can_ just skip this, but might as well validate while we're here
    {
        for _ in 0..body.read_var_u32()? {
            let pos = body.original_position();
            let count = body.read_var_u32()?;
            let ty = body.read_type()?;
            validator.define_locals(pos, count, ty)?;
        }
    }

    let function_address = body.original_position() as u64;

    while !body.eof() {
        let pos = body.original_position();
        let inst = body.read_operator()?;
        validator.op(pos, &inst)?;
    }

    validator.finish(body.original_position())?;

    Ok((
        function_address,
        body.original_position() as u64 - function_address,
    ))
}
