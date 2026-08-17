use gimli::{
    Encoding, Format, LineEncoding, LittleEndian,
    write::{
        Address, AttributeValue, DebuggingInformationEntry, DwarfUnit, EndianVec, FileId,
        LineProgram, LineString, Range, RangeList, RelocateWriter, Relocation, RelocationTarget,
        Sections, UnitEntryId, Writer,
    },
};
use symbolic_debuginfo::Object;

/// Record information needed to write a section.
#[derive(Clone)]
struct Section {
    data: EndianVec<LittleEndian>,
    relocations: Vec<Relocation>,
    id: Option<object::write::SectionId>,
}

impl Section {
    fn new() -> Self {
        Self {
            data: EndianVec::new(LittleEndian),
            relocations: Vec::new(),
            id: None,
        }
    }
}

impl RelocateWriter for Section {
    type Writer = EndianVec<LittleEndian>;

    fn writer(&self) -> &Self::Writer {
        &self.data
    }

    fn writer_mut(&mut self) -> &mut Self::Writer {
        &mut self.data
    }

    fn relocate(&mut self, relocation: Relocation) {
        self.relocations.push(relocation);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let binary_format = object::BinaryFormat::Elf;
    let mut obj = object::write::Object::new(
        binary_format,
        object::Architecture::X86_64,
        object::Endianness::Little,
    );

    let comp_dir = *b"/tmp";
    let file_name = *b"hello.c";
    let main_name = *b"main";

    // Addresses of the nested scopes, relative to the start of `main`.
    // main       [0x00, 0x40)
    //   nested   [0x10, 0x30)      a real (non-inlined) function nested in main
    //     inline [0x14, 0x28)      an inlined call inside nested
    //       leaf [0x18, 0x24)      a real function nested in the inlined call
    let symbol_relative_addr = |symbol: usize, addend: i64| Address::Symbol { symbol, addend };

    let (main_symbol, main_size) = define_main(&mut obj)?;
    let main_address = Address::Symbol {
        // This is a user defined identifier for the symbol.
        // In this case, we will use 0 to mean the main function.
        symbol: 0,
        addend: 0,
    };

    // Choose the encoding parameters.
    let encoding = Encoding {
        format: Format::Dwarf32,
        version: if binary_format == object::BinaryFormat::Coff {
            // The COFF toolchain I used didn't work with DWARF version 5.
            4
        } else {
            5
        },
        address_size: 8,
    };

    // Create a container for a single compilation unit.
    let mut dwarf = DwarfUnit::new(encoding);

    // Set attributes on the root DIE.
    let range_list_id = dwarf.unit.ranges.add(RangeList(vec![Range::StartLength {
        begin: main_address,
        length: main_size,
    }]));
    let root = dwarf.unit.root();
    let entry = dwarf.unit.get_mut(root);
    entry.set(
        gimli::DW_AT_producer,
        AttributeValue::String((*b"gimli example").into()),
    );
    entry.set(
        gimli::DW_AT_language,
        AttributeValue::Language(gimli::DW_LANG_C11),
    );
    entry.set(gimli::DW_AT_name, AttributeValue::String(file_name.into()));
    entry.set(
        gimli::DW_AT_comp_dir,
        AttributeValue::String(comp_dir.into()),
    );
    entry.set(gimli::DW_AT_low_pc, AttributeValue::Address(main_address));
    entry.set(
        gimli::DW_AT_ranges,
        AttributeValue::RangeListRef(range_list_id),
    );
    // DW_AT_stmt_list will be set automatically.

    // Add a line program for the main function.
    // For this example, we will only have one line in the line program.
    let line_strings = &mut dwarf.line_strings;
    let mut line_program = LineProgram::new(
        encoding,
        LineEncoding::default(),
        LineString::new(comp_dir, encoding, line_strings),
        None,
        LineString::new(file_name, encoding, line_strings),
        None,
    );
    let dir_id = line_program.default_directory();
    let file_string = LineString::new(file_name, encoding, line_strings);
    let file_id = line_program.add_file(file_string, dir_id, None);
    // One row per scope boundary, so every function and the inlinee get line records.
    // line_program.begin_sequence(Some(main_address));
    // for (offset, line) in [
    //     (0x00, 2),  // main
    //     (0x10, 10), // nested_fn
    //     (0x14, 20), // inlined_fn (inlined at hello.c:12)
    //     (0x18, 22), // innermost_fn
    //     (0x24, 21), // back in inlined_fn
    //     (0x28, 13), // back in nested_fn
    //     (0x30, 4),  // back in main
    // ] {
    //     line_program.row().address_offset = offset;
    //     line_program.row().file = file_id;
    //     line_program.row().line = line;
    //     line_program.generate_row();
    // }
    // line_program.end_sequence(main_size);
    //dwarf.unit.line_program = line_program;

    // Add a subprogram DIE for the main function.
    // Note that this example does not include all attributes.

    let entry = add_function(
        &mut dwarf,
        file_id,
        root,
        "main",
        2,
        main_address,
        main_size,
    );
    entry.set(gimli::DW_AT_external, AttributeValue::Flag(true));
    let subprogram = entry.id();

    // The abstract instance of the function that gets inlined further down. It carries no
    // ranges of its own; the concrete `DW_TAG_inlined_subroutine` points at it via
    // `DW_AT_abstract_origin` and that is where its name comes from.
    let entry = add_inline_function_decl(&mut dwarf, file_id, root, "inlined_fn", 20);

    let inline_abstract = entry.id();

    // A real function nested inside `main`. symbolic reports this as its own top-level
    // function, not as part of `main`.
    let entry = add_function(
        &mut dwarf,
        file_id,
        subprogram,
        "nested_fn",
        10,
        Address::Constant(0x10),
        0x20,
    );
    let nested = entry.id();

    // );
    // entry.set(gimli::DW_AT_high_pc, AttributeValue::Udata(0x20));

    // An inlined call inside `nested_fn`. This becomes an inlinee of `nested_fn`.
    let entry = add_abstract_inline_function(
        &mut dwarf,
        file_id,
        nested,
        inline_abstract,
        Address::Constant(0x14),
        0x14,
    );
    let inlined = entry.id();

    // A real function nested inside the inlined call.
    add_function(
        &mut dwarf,
        file_id,
        inlined,
        "innermost_fn",
        22,
        Address::Constant(0x18),
        0xc,
    );

    // Build the DWARF sections.
    // This will populate the sections with the DWARF data and relocations.
    let mut sections = Sections::new(Section::new());
    dwarf.write(&mut sections)?;

    // Add the DWARF section data to the object file.
    sections.for_each_mut(|id, section| -> object::write::Result<()> {
        if section.data.len() == 0 {
            return Ok(());
        }
        let kind = if id.is_string() {
            object::SectionKind::DebugString
        } else {
            object::SectionKind::Debug
        };
        let section_id = obj.add_section(Vec::new(), id.name().into(), kind);
        obj.set_section_data(section_id, section.data.take(), 1);

        // Record the section ID so that it can be used for relocations.
        section.id = Some(section_id);
        Ok(())
    })?;

    // Add the relocations to the object file.
    // sections.for_each(|_, section| -> object::write::Result<()> {
    //     let Some(section_id) = section.id else {
    //         debug_assert!(section.relocations.is_empty());
    //         return Ok(());
    //     };
    //     for reloc in &section.relocations {
    //         // The `eh_pe` field is not used in this example because we are not writing
    //         // unwind information.
    //         debug_assert!(reloc.eh_pe.is_none());
    //         let (symbol, kind) = match reloc.target {
    //             RelocationTarget::Section(id) => {
    //                 let kind = if binary_format == object::BinaryFormat::Coff {
    //                     object::RelocationKind::SectionOffset
    //                 } else {
    //                     object::RelocationKind::Absolute
    //                 };
    //                 let symbol = obj.section_symbol(sections.get(id).unwrap().id.unwrap());
    //                 (symbol, kind)
    //             }
    //             RelocationTarget::Symbol(id) => {
    //                 // The main function is the only symbol we have defined.
    //                 debug_assert_eq!(id, 0);
    //                 (main_symbol, object::RelocationKind::Absolute)
    //             }
    //         };
    //         obj.add_relocation(
    //             section_id,
    //             object::write::Relocation {
    //                 offset: reloc.offset as u64,
    //                 symbol,
    //                 addend: reloc.addend,
    //                 flags: object::RelocationFlags::Generic {
    //                     kind,
    //                     encoding: object::RelocationEncoding::Generic,
    //                     size: reloc.size * 8,
    //                 },
    //             },
    //         )?;
    //     }
    //     Ok(())
    // })?;

    let obj = obj.write()?;

    let parsed = Object::parse(&obj)?;

    dbg!(&parsed);

    let session = parsed.debug_session()?;

    for f in session.functions() {
        let f = f.unwrap();
        dbg!(f);
    }

    Ok(())
}

fn add_inline_function_decl<'a>(
    dwarf: &'a mut DwarfUnit,
    file: FileId,
    parent: UnitEntryId,
    name: &str,
    decl_line: u64,
) -> &'a mut DebuggingInformationEntry {
    let uie = dwarf.unit.add(parent, gimli::DW_TAG_subprogram);
    let entry = dwarf.unit.get_mut(uie);
    entry.set(gimli::DW_AT_name, AttributeValue::String(name.into()));
    entry.set(
        gimli::DW_AT_inline,
        AttributeValue::Inline(gimli::DW_INL_inlined),
    );
    entry.set(
        gimli::DW_AT_decl_file,
        AttributeValue::FileIndex(Some(file)),
    );
    entry.set(gimli::DW_AT_decl_line, AttributeValue::Udata(decl_line));

    entry
}

fn add_abstract_inline_function<'a>(
    dwarf: &'a mut DwarfUnit,
    file: FileId,
    parent: UnitEntryId,
    decl_ref: UnitEntryId,
    low_pc: Address,
    high_pc_offset: u64,
) -> &'a mut DebuggingInformationEntry {
    let uie = dwarf.unit.add(parent, gimli::DW_TAG_inlined_subroutine);
    let entry = dwarf.unit.get_mut(uie);
    entry.set(
        gimli::DW_AT_abstract_origin,
        AttributeValue::UnitRef(decl_ref),
    );
    entry.set(
        gimli::DW_AT_call_file,
        AttributeValue::FileIndex(Some(file)),
    );
    entry.set(gimli::DW_AT_low_pc, AttributeValue::Address(low_pc));
    entry.set(gimli::DW_AT_high_pc, AttributeValue::Udata(high_pc_offset));

    entry
}

fn add_function<'a>(
    dwarf: &'a mut DwarfUnit,
    file: FileId,
    parent: UnitEntryId,
    name: &str,
    decl_line: u64,
    low_pc: Address,
    high_pc_offset: u64,
) -> &'a mut DebuggingInformationEntry {
    let uei = dwarf.unit.add(parent, gimli::DW_TAG_subprogram);
    let entry = dwarf.unit.get_mut(uei);
    entry.set(gimli::DW_AT_name, AttributeValue::String(name.into()));
    entry.set(
        gimli::DW_AT_decl_file,
        AttributeValue::FileIndex(Some(file)),
    );
    entry.set(gimli::DW_AT_decl_line, AttributeValue::Udata(decl_line));
    entry.set(gimli::DW_AT_low_pc, AttributeValue::Address(low_pc));
    entry.set(gimli::DW_AT_high_pc, AttributeValue::Udata(high_pc_offset));

    entry
}

fn define_main(
    obj: &mut object::write::Object,
) -> Result<(object::write::SymbolId, u64), Box<dyn std::error::Error>> {
    // Add a file symbol (STT_FILE or equivalent).
    obj.add_file_symbol((*b"hello.c").into());

    // Generate code for the equivalent of this C function:
    //     int main() {
    //         puts("Hello, world!");
    //         return 0;
    //     }
    // let mut main_data = Vec::new();
    // // sub $0x28, %rsp
    // main_data.extend_from_slice(&[0x48, 0x83, 0xec, 0x28]);
    // // Handle different calling convention on Windows.
    // if cfg!(target_os = "windows") {
    //     // lea 0x0(%rip), %rcx
    //     main_data.extend_from_slice(&[0x48, 0x8d, 0x0d, 0x00, 0x00, 0x00, 0x00]);
    // } else {
    //     // lea 0x0(%rip), %rdi
    //     main_data.extend_from_slice(&[0x48, 0x8d, 0x3d, 0x00, 0x00, 0x00, 0x00]);
    // }
    // // R_X86_64_PC32 .rodata-0x4
    // let s_reloc_offset = main_data.len() - 4;
    // let s_reloc_addend = -4;
    // let s_reloc_flags = object::RelocationFlags::Generic {
    //     kind: object::RelocationKind::Relative,
    //     encoding: object::RelocationEncoding::Generic,
    //     size: 32,
    // };
    // // call 14 <main+0x14>
    // main_data.extend_from_slice(&[0xe8, 0x00, 0x00, 0x00, 0x00]);
    // // R_X86_64_PLT32 puts-0x4
    // let puts_reloc_offset = main_data.len() - 4;
    // let puts_reloc_addend = -4;
    // let puts_reloc_flags = object::RelocationFlags::Generic {
    //     kind: object::RelocationKind::PltRelative,
    //     encoding: object::RelocationEncoding::X86Branch,
    //     size: 32,
    // };
    // // xor %eax, %eax
    // main_data.extend_from_slice(&[0x31, 0xc0]);
    // // add $0x28, %rsp
    // main_data.extend_from_slice(&[0x48, 0x83, 0xc4, 0x28]);
    // // ret
    // main_data.extend_from_slice(&[0xc3]);

    // Add a globally visible symbol for the main function.
    let main_symbol = obj.add_symbol(object::write::Symbol {
        name: (*b"main").into(),
        value: 0,
        size: 0,
        kind: object::SymbolKind::Text,
        scope: object::SymbolScope::Linkage,
        weak: false,
        section: object::write::SymbolSection::Undefined,
        flags: object::SymbolFlags::None,
    });
    // Add the main function in its own subsection (equivalent to -ffunction-sections).
    //let main_section = obj.add_subsection(object::write::StandardSection::Text, b"main");
    //let main_offset = obj.add_symbol_data(main_symbol, main_section, &main_data, 1);

    // Add a read only string constant for the puts argument.
    // We don't create a symbol for the constant, but instead refer to it by
    // the section symbol and section offset.
    //let rodata_section = obj.section_id(object::write::StandardSection::ReadOnlyData);
    //let rodata_symbol = obj.section_symbol(rodata_section);
    //let s_offset = obj.append_section_data(rodata_section, b"Hello, world!\0", 1);

    // Relocation for the string constant.
    // obj.add_relocation(
    //     main_section,
    //     object::write::Relocation {
    //         offset: main_offset + s_reloc_offset as u64,
    //         symbol: rodata_symbol,
    //         addend: s_offset as i64 + s_reloc_addend,
    //         flags: s_reloc_flags,
    //     },
    // )?;

    // External symbol for puts.
    // let puts_symbol = obj.add_symbol(object::write::Symbol {
    //     name: (*b"puts").into(),
    //     value: 0,
    //     size: 0,
    //     kind: object::SymbolKind::Text,
    //     scope: object::SymbolScope::Dynamic,
    //     weak: false,
    //     section: object::write::SymbolSection::Undefined,
    //     flags: object::SymbolFlags::None,
    // });

    // Relocation for the call to puts.
    // obj.add_relocation(
    //     main_section,
    //     object::write::Relocation {
    //         offset: puts_reloc_offset as u64,
    //         symbol: puts_symbol,
    //         addend: puts_reloc_addend,
    //         flags: puts_reloc_flags,
    //     },
    // )?;

    Ok((main_symbol, 0x40))
}
