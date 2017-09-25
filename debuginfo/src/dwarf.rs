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
    pub fn get_elf_section(&self) -> &'static str {
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
    pub fn get_macho_section(&self) -> &'static str {
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
