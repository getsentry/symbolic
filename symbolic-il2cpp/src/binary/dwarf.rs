use std::collections::BTreeMap;

#[derive(Debug)]
pub struct DwarfData {
    /// A map from function pointer (DW_AT_low_pc) to function name (DW_AT_name).
    pub functions: BTreeMap<u64, String>,
    /// The offset of `g_CodeGenModules` (DW_TAG_variable) in the corresponding executable file.
    pub codegenmodules_offset: Option<u64>,
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
