use std::collections::HashMap;

use scroll::Pread;

mod dwarf;
mod executable;

pub use dwarf::DwarfData;
pub use executable::Il2CppCodeGenModule;

pub fn build_native_method_map(
    binary_buf: &[u8],
    dwarf_data: DwarfData,
) -> anyhow::Result<HashMap<String, HashMap<usize, String>>> {
    let mut method_map = HashMap::new();

    // TODO: for each codegen module
    let codegenmodules_offset = dwarf_data.codegenmodules_offset.unwrap() as usize;
    {
        // TODO: make this ptr-size / endian aware!
        let assembly_in_modules = binary_buf.pread::<u64>(codegenmodules_offset)? as usize;
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
