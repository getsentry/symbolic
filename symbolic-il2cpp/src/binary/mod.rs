use std::collections::HashMap;

use scroll::Pread;

mod code_registration;
mod dwarf;
mod executable;

pub use code_registration::CodeRegistration;
pub use dwarf::DwarfData;
pub use executable::Il2CppCodeGenModule;

/// Parser Context that is being used for parsing structs out of the binaries static data sections
#[derive(Clone, Copy, Debug)]
pub(crate) struct BinaryCtx {
    // TODO: make sure we have pointer-width and endianness aware parsers
}

pub fn build_native_method_map(
    binary_buf: &[u8],
    dwarf_data: DwarfData,
) -> anyhow::Result<HashMap<String, HashMap<usize, String>>> {
    let mut method_map = HashMap::new();

    let offset = &mut (dwarf_data
        .code_registration_offset
        .ok_or_else(|| anyhow::Error::msg("expected a code registration offset"))?
        as usize);

    let code_registration: CodeRegistration = binary_buf.gread_with(offset, BinaryCtx {})?;

    for i in 0..code_registration.codegen_modules_len as usize {
        // TODO: make this ptr-size / endian aware!
        let codegen_module_offset =
            code_registration.codegen_modules_ptr as usize + std::mem::size_of::<u64>() * i;
        let assembly_in_modules = binary_buf.pread::<u64>(codegen_module_offset)? as usize;
        let module = Il2CppCodeGenModule::parse(binary_buf, assembly_in_modules)?;

        let mut indexed_functions = HashMap::new();

        for (i, fn_ptr) in module.method_pointers.iter().enumerate() {
            if let Some(fn_name) = dwarf_data.functions.get(fn_ptr) {
                indexed_functions.insert(i, fn_name.to_owned());
            }
        }

        if !indexed_functions.is_empty() {
            method_map.insert(module.name.to_string(), indexed_functions);
        }
    }

    Ok(method_map)
}
