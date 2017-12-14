use goblin::{elf, mach};

use object::{Object, ObjectTarget};

pub trait DwarfData {
    /// Loads a specific dwarf section if its in the file.
    fn get_dwarf_section<'input>(
        &'input self,
        section: DwarfSection,
    ) -> Option<DwarfSectionData<'input>>;
}

impl<'input> DwarfData for Object<'input> {
    fn get_dwarf_section<'data>(
        &'data self,
        section: DwarfSection,
    ) -> Option<DwarfSectionData<'data>> {
        match self.target {
            ObjectTarget::Elf(ref elf) => read_elf_dwarf_section(elf, self.as_bytes(), section),
            ObjectTarget::MachOSingle(macho) => read_macho_dwarf_section(macho, section),
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
pub struct DwarfSectionData<'a> {
    section: DwarfSection,
    data: &'a [u8],
    offset: u64,
}

impl<'a> DwarfSectionData<'a> {
    pub fn new(section: DwarfSection, data: &'a [u8], offset: u64) -> DwarfSectionData<'a> {
        DwarfSectionData {
            section: section,
            data: data,
            offset: offset,
        }
    }

    /// Return the section as bytes
    pub fn as_bytes(&self) -> &'a [u8] {
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

fn read_elf_dwarf_section<'a>(
    elf: &elf::Elf<'a>,
    data: &'a [u8],
    sect: DwarfSection,
) -> Option<DwarfSectionData<'a>> {
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

fn read_macho_dwarf_section<'a>(
    macho: &mach::MachO<'a>,
    sect: DwarfSection,
) -> Option<DwarfSectionData<'a>> {
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
                            return Some(DwarfSectionData::new(
                                sect, data, section.offset as u64));
                        }
                    }
                }
            }
        }
    }

    None
}
