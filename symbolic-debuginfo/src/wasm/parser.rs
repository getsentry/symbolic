//! Contains utilities for parsing a WASM module to retrieve the information needed by [`super::WasmObject`]

use super::WasmError;
use crate::base::{ObjectKind, Symbol};
use wasmparser::{Payload, TypeRef, Validator, WasmFeatures};

impl<'data> super::WasmObject<'data> {
    /// Tries to parse a WASM from the given slice.
    pub fn parse(data: &'data [u8]) -> Result<Self, WasmError> {
        let mut code_offset = 0;
        let mut build_id = None;
        let mut dwarf_sections = Vec::new();
        let mut kind = ObjectKind::Debug;

        // In "normal" wasm modules the only types will be function signatures, but in the future it
        // could contain types used for module linking, but we don't actually care about the types,
        // just that the function references a valid signature, so we just keep a bitset of the function
        // signatures to verify that
        let mut func_sigs = bitvec::vec::BitVec::<usize, bitvec::order::Lsb0>::new();
        // NOTE: make sure to update these when bumping the `wasmparser` depedency.
        let features = WasmFeatures {
            relaxed_simd: true,
            threads: true,
            tail_call: true,
            multi_memory: true,
            exceptions: true,
            memory64: true,
            extended_const: true,
            component_model: true,
            ..Default::default()
        };
        let mut validator = Validator::new_with_features(features);
        let mut funcs = Vec::<Symbol>::new();
        let mut num_imported_funcs = 0u32;

        // Parse the wasm file to pull out the function and their starting address, size, and name
        // Note that the order of the payloads here are the order that they will appear in (valid)
        // wasm binaries, other than the sections that we need to parse to validate the module, which
        // are at the end
        for payload in wasmparser::Parser::new(0).parse_all(data) {
            let payload = payload?;
            match payload {
                // The type section contains, well, types, specifically, function signatures that are
                // later referenced by the function section.
                Payload::TypeSection(tsr) => {
                    validator.type_section(&tsr)?;
                    func_sigs.resize(tsr.get_count() as usize, false);
                    let fs = func_sigs.as_mut_bitslice();

                    for (i, ty) in tsr.into_iter().enumerate() {
                        if matches!(ty?, wasmparser::TypeDef::Func(_)) {
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
                        if let TypeRef::Func(id) = import.ty {
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

                    funcs.reserve(fsr.get_count() as usize);

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
                    let validator = validator.code_section_entry(&body)?;

                    let (address, size) = get_function_info(body, validator)?;

                    // Though we have an accurate? size of the function body, the old method of symbol
                    // iterating with walrus extends the size of each body to be contiguous with the
                    // next function, so we do the same, other than the final function
                    if let Some(prev) = funcs.last_mut() {
                        prev.size = address - prev.address;
                    }

                    funcs.push(Symbol {
                        name: None,
                        address,
                        size,
                    });
                }

                Payload::ModuleSection { range, .. } => {
                    validator.module_section(&range)?;
                }
                // There are several custom sections that we need
                Payload::CustomSection(reader) => {
                    match reader.name() {
                        // this section is not defined yet
                        // see https://github.com/WebAssembly/tool-conventions/issues/133
                        "build_id" => {
                            build_id = Some(reader.data());
                        }
                        // All of the dwarf debug sections (.debug_frame, .debug_info etc) start with a `.`, and
                        // are the only ones we need for walking the debug info
                        debug if debug.starts_with('.') => {
                            dwarf_sections.push((debug, reader.data()));
                        }
                        // The name section contains the symbol names for items, notably functions
                        "name" => {
                            let nsr = wasmparser::NameSectionReader::new(
                                reader.data(),
                                reader.data_offset(),
                            )?;

                            for name in nsr {
                                if let wasmparser::Name::Function(fnames) = name? {
                                    let mut map = fnames.get_map()?;
                                    for _ in 0..map.get_count() {
                                        let fname = map.read()?;

                                        // The names for imported functions are also in this table, but
                                        // we don't care about them
                                        if fname.index >= num_imported_funcs {
                                            if let Some(func) = funcs.get_mut(
                                                (fname.index - num_imported_funcs) as usize,
                                            ) {
                                                func.name =
                                                    Some(std::borrow::Cow::Borrowed(fname.name));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }

                // All other sections are not used by this crate, but some (eg table/memory/global)
                // are needed to validate the sections that we do care about, so we just validate all
                // of the payloads we don't use to be sure
                payload => {
                    validator.payload(&payload)?;
                }
            }
        }

        Ok(Self {
            dwarf_sections,
            funcs,
            build_id,
            data,
            code_offset,
            kind,
        })
    }
}

fn get_function_info(
    body: wasmparser::FunctionBody,
    mut validator: wasmparser::FuncValidator<wasmparser::ValidatorResources>,
) -> Result<(u64, u64), WasmError> {
    let mut body = body.get_binary_reader();

    let function_address = body.original_position() as u64;

    // locals, we _can_ just skip this, but might as well validate while we're here
    {
        for _ in 0..body.read_var_u32()? {
            let pos = body.original_position();
            let count = body.read_var_u32()?;
            let ty = body.read_type()?;
            validator.define_locals(pos, count, ty)?;
        }
    }

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
