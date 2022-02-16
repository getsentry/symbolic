use scroll::Pread;

use crate::utils::CStrCtx;

const IL2CPP_METADATA_MAGIC: u32 = 0xFAB11BAF; // TODO: use from_bytes_be

#[derive(Debug)]
pub struct Il2CppMetadata {
    pub version: u32,
}

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

        // now follow offset + size pairs (each u32):
        let _str_literal_offset: u32 = buf.gread(offset)?;
        let _str_literal_size: u32 = buf.gread(offset)?;

        let _str_literal_data_offset: u32 = buf.gread(offset)?;
        let _str_literal_data_size: u32 = buf.gread(offset)?;

        let str_data_offset = buf.gread::<u32>(offset)? as usize;
        let str_data_size = buf.gread::<u32>(offset)? as usize;
        let str_data = &buf[str_data_offset..(str_data_offset + str_data_size)];
        let str_at = |idx: u32| -> Result<&str, _> { str_data.pread_with(idx as usize, CStrCtx) };

        let _events_offset: u32 = buf.gread(offset)?;
        let _events_size: u32 = buf.gread(offset)?;

        let _properties_offset: u32 = buf.gread(offset)?;
        let _properties_size: u32 = buf.gread(offset)?;

        let _methods_offset: u32 = buf.gread(offset)?;
        let _methods_size: u32 = buf.gread(offset)?;

        let _param_default_values_offset: u32 = buf.gread(offset)?;
        let _param_default_values_size: u32 = buf.gread(offset)?;

        let _field_default_values_offset: u32 = buf.gread(offset)?;
        let _field_default_values_size: u32 = buf.gread(offset)?;

        let _field_and_param_default_values_offset: u32 = buf.gread(offset)?;
        let _field_and_param_default_values_size: u32 = buf.gread(offset)?;

        let _field_marshaled_sizes_offset: u32 = buf.gread(offset)?;
        let _field_marshaled_sizes_size: u32 = buf.gread(offset)?;

        let _parameters_offset: u32 = buf.gread(offset)?;
        let _parameters_size: u32 = buf.gread(offset)?;

        let _fields_offset: u32 = buf.gread(offset)?;
        let _fields_size: u32 = buf.gread(offset)?;

        let _generic_parameters_offset: u32 = buf.gread(offset)?;
        let _generic_parameters_size: u32 = buf.gread(offset)?;

        let _generic_parameter_constraints_offset: u32 = buf.gread(offset)?;
        let _generic_parameter_constraints_size: u32 = buf.gread(offset)?;

        let _generic_containers_offset: u32 = buf.gread(offset)?;
        let _generic_containers_size: u32 = buf.gread(offset)?;

        let _nested_types_offset: u32 = buf.gread(offset)?;
        let _nested_types_size: u32 = buf.gread(offset)?;

        let _interfaces_offset: u32 = buf.gread(offset)?;
        let _interfaces_size: u32 = buf.gread(offset)?;

        let _vtable_methods_offset: u32 = buf.gread(offset)?;
        let _vtable_methods_size: u32 = buf.gread(offset)?;

        let _interface_offsets_offset: u32 = buf.gread(offset)?;
        let _interface_offsets_size: u32 = buf.gread(offset)?;

        let _type_definitions_offset: u32 = buf.gread(offset)?;
        let _type_definitions_size: u32 = buf.gread(offset)?;

        let _images_offset: u32 = buf.gread(offset)?;
        let _images_size: u32 = buf.gread(offset)?;

        let _assemblies_offset: u32 = buf.gread(offset)?;
        let _assemblies_size: u32 = buf.gread(offset)?;

        // also:
        // * field refs
        // * referenced assemblies
        // * attribute data
        // * attribute data range
        // * unresolved virtual call parameter types
        // * unresolved virtual call parameter ranges
        // * windows runtime type names
        // * windows runtime strings
        // * exported type definitions

        dbg!(
            _methods_offset,
            _methods_size,
            _images_offset,
            _images_size,
            _assemblies_offset,
            _assemblies_size
        );

        // let sizeof_image = 10 * 4;

        // for idx in 0..(_images_size as usize / sizeof_image) {
        //     let image_offset = _images_offset as usize + idx * sizeof_image;

        //     dbg!(image_offset, str_at(buf.pread(image_offset)?));
        // }

        Ok(Self { version })
    }
}

#[derive(Debug)]
pub struct Il2CppMethodDefinition {
    name_idx: u32,
    declaring_type_idx: u32,
    return_type_idx: u32,
}

impl Il2CppMethodDefinition {
    // pub fn parse()
}
