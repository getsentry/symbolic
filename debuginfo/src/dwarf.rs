use goblin::{elf, mach};

use object::{Object, ObjectTarget};

pub trait DwarfData {
    /// Checks whether this object contains DWARF infos.
    fn has_dwarf_data(&self) -> bool;

    /// Loads a specific dwarf section if its in the file.
    fn get_dwarf_section<'input>(
        &'input self,
        section: DwarfSection,
    ) -> Option<DwarfSectionData<'input>>;
}

impl<'input> DwarfData for Object<'input> {
    fn has_dwarf_data(&self) -> bool {
        match self.target {
            // We assume an ELF contains debug information if it still contains
            // the debug_info section. The file utility uses a similar mechanism,
            // except that it checks for the ".symtab" section instead.
            ObjectTarget::Elf(ref elf) => has_elf_section(elf, DwarfSection::DebugInfo),

            // MachO generally stores debug information in the "__DWARF" segment,
            // so we simply check if it is present. The only exception to this
            // rule is call frame information (CFI), which is stored in the __TEXT
            // segment of the executable. This, however, requires more specific
            // logic anyway, so we ignore this here.
            ObjectTarget::MachOSingle(ref macho) => has_macho_segment(macho, "__DWARF"),
            ObjectTarget::MachOFat(_, ref macho) => has_macho_segment(macho, "__DWARF"),

            // We do not support DWARF in any other object targets
            _ => false,
        }
    }

    fn get_dwarf_section<'data>(
        &'data self,
        section: DwarfSection,
    ) -> Option<DwarfSectionData<'data>> {
        match self.target {
            ObjectTarget::Elf(ref elf) => read_elf_dwarf_section(elf, self.as_bytes(), section),
            ObjectTarget::MachOSingle(ref macho) => read_macho_dwarf_section(macho, section),
            ObjectTarget::MachOFat(_, ref macho) => read_macho_dwarf_section(macho, section),
            _ => None,
        }
    }
}

/// Represents the name of the section.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub enum DwarfSection {
    EhFrame,
    DebugFrame,
    DebugAbbrev,
    DebugAranges,
    DebugLine,
    DebugLoc,
    DebugPubNames,
    DebugRanges,
    DebugStr,
    DebugInfo,
    DebugTypes,
}

impl DwarfSection {
    /// Return the name for elf
    pub fn elf_name(&self) -> &'static str {
        match *self {
            DwarfSection::EhFrame => ".eh_frame",
            DwarfSection::DebugFrame => ".debug_frame",
            DwarfSection::DebugAbbrev => ".debug_abbrev",
            DwarfSection::DebugAranges => ".debug_aranges",
            DwarfSection::DebugLine => ".debug_line",
            DwarfSection::DebugLoc => ".debug_loc",
            DwarfSection::DebugPubNames => ".debug_pubnames",
            DwarfSection::DebugRanges => ".debug_ranges",
            DwarfSection::DebugStr => ".debug_str",
            DwarfSection::DebugInfo => ".debug_info",
            DwarfSection::DebugTypes => ".debug_types",
        }
    }

    /// Return the name for macho
    pub fn macho_name(&self) -> &'static str {
        match *self {
            DwarfSection::EhFrame => "__eh_frame",
            DwarfSection::DebugFrame => "__debug_frame",
            DwarfSection::DebugAbbrev => "__debug_abbrev",
            DwarfSection::DebugAranges => "__debug_aranges",
            DwarfSection::DebugLine => "__debug_line",
            DwarfSection::DebugLoc => "__debug_loc",
            DwarfSection::DebugPubNames => "__debug_pubnames",
            DwarfSection::DebugRanges => "__debug_ranges",
            DwarfSection::DebugStr => "__debug_str",
            DwarfSection::DebugInfo => "__debug_info",
            DwarfSection::DebugTypes => "__debug_types",
        }
    }

    /// Return the name of the section for debug purposes
    pub fn name(&self) -> &'static str {
        match *self {
            DwarfSection::EhFrame => "eh_frame",
            DwarfSection::DebugFrame => "debug_frame",
            DwarfSection::DebugAbbrev => "debug_abbrev",
            DwarfSection::DebugAranges => "debug_aranges",
            DwarfSection::DebugLine => "debug_line",
            DwarfSection::DebugLoc => "debug_loc",
            DwarfSection::DebugPubNames => "debug_pubnames",
            DwarfSection::DebugRanges => "debug_ranges",
            DwarfSection::DebugStr => "debug_str",
            DwarfSection::DebugInfo => "debug_info",
            DwarfSection::DebugTypes => "debug_types",
        }
    }
}

/// Gives access to a section in a dwarf file.
#[derive(Eq, PartialEq, Debug, Clone)]
pub struct DwarfSectionData<'data> {
    section: DwarfSection,
    data: &'data [u8],
    offset: u64,
}

impl<'data> DwarfSectionData<'data> {
    pub fn new(section: DwarfSection, data: &'data [u8], offset: u64) -> DwarfSectionData<'data> {
        DwarfSectionData {
            section: section,
            data: data,
            offset: offset,
        }
    }

    /// Return the section as bytes
    pub fn as_bytes(&self) -> &'data [u8] {
        self.data
    }

    /// Get the offset
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Get the section
    pub fn section(&self) -> DwarfSection {
        self.section
    }
}

fn read_elf_dwarf_section<'data>(
    elf: &elf::Elf<'data>,
    data: &'data [u8],
    sect: DwarfSection,
) -> Option<DwarfSectionData<'data>> {
    let section_name = sect.elf_name();

    for header in &elf.section_headers {
        if let Some(Ok(name)) = elf.shdr_strtab.get(header.sh_name) {
            if name == section_name {
                let sec_data = &data[header.sh_offset as usize..][..header.sh_size as usize];
                return Some(DwarfSectionData::new(sect, sec_data, header.sh_offset));
            }
        }
    }

    None
}

fn read_macho_dwarf_section<'data>(
    macho: &mach::MachO<'data>,
    sect: DwarfSection,
) -> Option<DwarfSectionData<'data>> {
    let dwarf_segment = if sect == DwarfSection::EhFrame {
        "__TEXT"
    } else {
        "__DWARF"
    };

    let dwarf_section_name = sect.macho_name();
    for segment in &macho.segments {
        if_chain! {
            if let Ok(seg) = segment.name();
            if dwarf_segment == seg;
            then {
                for section in segment {
                    if_chain! {
                        if let Ok((section, data)) = section;
                        if let Ok(name) = section.name();
                        if name == dwarf_section_name;
                        then {
                            return Some(DwarfSectionData::new(sect, data, section.offset as u64));
                        }
                    }
                }
            }
        }
    }

    None
}

fn has_elf_section(elf: &elf::Elf, section: DwarfSection) -> bool {
    for header in &elf.section_headers {
        if let Some(Ok(name)) = elf.shdr_strtab.get(header.sh_name) {
            if name == section.elf_name() {
                return true;
            }
        }
    }

    false
}

fn has_macho_segment(macho: &mach::MachO, name: &str) -> bool {
    for segment in &macho.segments {
        if segment.name().map(|seg| seg == name).unwrap_or(false) {
            return true;
        }
    }

    false
}
