use goblin::{elf, mach};

use crate::elf::{find_elf_section, has_elf_section};
use crate::mach::{find_mach_section, has_mach_segment};
use crate::object::{Object, ObjectTarget};

/// Provides access to DWARF debugging information in object files.
pub trait DwarfData {
    /// Checks whether this object contains DWARF infos.
    fn has_dwarf_data(&self) -> bool;

    /// Loads a specific dwarf section if its in the file.
    fn get_dwarf_section(&self, section: DwarfSection) -> Option<DwarfSectionData>;
}

impl<'input> DwarfData for Object<'input> {
    fn has_dwarf_data(&self) -> bool {
        match self.target {
            // We assume an ELF contains debug information if it still contains
            // the debug_info section. The file utility uses a similar mechanism,
            // except that it checks for the ".symtab" section instead.
            ObjectTarget::Elf(ref elf) => has_elf_section(
                elf,
                elf::section_header::SHT_PROGBITS,
                DwarfSection::DebugInfo.elf_name(),
            ),

            // MachO generally stores debug information in the "__DWARF" segment,
            // so we simply check if it is present. The only exception to this
            // rule is call frame information (CFI), which is stored in the __TEXT
            // segment of the executable. This, however, requires more specific
            // logic anyway, so we ignore this here.
            ObjectTarget::MachOSingle(ref macho) => has_mach_segment(macho, "__DWARF"),
            ObjectTarget::MachOFat(_, ref macho) => has_mach_segment(macho, "__DWARF"),

            // We do not support DWARF in any other object targets
            _ => false,
        }
    }

    fn get_dwarf_section(&self, section: DwarfSection) -> Option<DwarfSectionData> {
        match self.target {
            ObjectTarget::Elf(ref elf) => read_elf_dwarf_section(elf, self.as_bytes(), section),
            ObjectTarget::MachOSingle(ref macho) => read_mach_dwarf_section(macho, section),
            ObjectTarget::MachOFat(_, ref macho) => read_mach_dwarf_section(macho, section),
            _ => None,
        }
    }
}

/// Represents the name of a DWARF debug section.
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
    DebugRngLists,
    DebugStr,
    DebugInfo,
    DebugTypes,
}

impl DwarfSection {
    /// Return the name for ELF.
    pub fn elf_name(self) -> &'static str {
        match self {
            DwarfSection::EhFrame => ".eh_frame",
            DwarfSection::DebugFrame => ".debug_frame",
            DwarfSection::DebugAbbrev => ".debug_abbrev",
            DwarfSection::DebugAranges => ".debug_aranges",
            DwarfSection::DebugLine => ".debug_line",
            DwarfSection::DebugLoc => ".debug_loc",
            DwarfSection::DebugPubNames => ".debug_pubnames",
            DwarfSection::DebugRanges => ".debug_ranges",
            DwarfSection::DebugRngLists => ".debug_rnglists",
            DwarfSection::DebugStr => ".debug_str",
            DwarfSection::DebugInfo => ".debug_info",
            DwarfSection::DebugTypes => ".debug_types",
        }
    }

    /// Return the name for MachO.
    pub fn macho_name(self) -> &'static str {
        match self {
            DwarfSection::EhFrame => "__eh_frame",
            DwarfSection::DebugFrame => "__debug_frame",
            DwarfSection::DebugAbbrev => "__debug_abbrev",
            DwarfSection::DebugAranges => "__debug_aranges",
            DwarfSection::DebugLine => "__debug_line",
            DwarfSection::DebugLoc => "__debug_loc",
            DwarfSection::DebugPubNames => "__debug_pubnames",
            DwarfSection::DebugRanges => "__debug_ranges",
            DwarfSection::DebugRngLists => "__debug_rnglists",
            DwarfSection::DebugStr => "__debug_str",
            DwarfSection::DebugInfo => "__debug_info",
            DwarfSection::DebugTypes => "__debug_types",
        }
    }

    /// Return the name of the section for debug purposes.
    pub fn name(self) -> &'static str {
        match self {
            DwarfSection::EhFrame => "eh_frame",
            DwarfSection::DebugFrame => "debug_frame",
            DwarfSection::DebugAbbrev => "debug_abbrev",
            DwarfSection::DebugAranges => "debug_aranges",
            DwarfSection::DebugLine => "debug_line",
            DwarfSection::DebugLoc => "debug_loc",
            DwarfSection::DebugPubNames => "debug_pubnames",
            DwarfSection::DebugRanges => "debug_ranges",
            DwarfSection::DebugRngLists => "debug_rnglists",
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
    /// Constructs a `DwarfSectionData` object from raw data.
    pub fn new(section: DwarfSection, data: &[u8], offset: u64) -> DwarfSectionData {
        DwarfSectionData {
            section,
            data,
            offset,
        }
    }

    /// Return the section data as bytes.
    pub fn as_bytes(&self) -> &'data [u8] {
        self.data
    }

    /// Get the absolute file offset.
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Get the section name.
    pub fn section(&self) -> DwarfSection {
        self.section
    }
}

/// Reads a single `DwarfSection` from an ELF object file.
fn read_elf_dwarf_section<'data>(
    elf: &elf::Elf<'data>,
    data: &'data [u8],
    sect: DwarfSection,
) -> Option<DwarfSectionData<'data>> {
    let sh_type = elf::section_header::SHT_PROGBITS;
    find_elf_section(elf, data, sh_type, sect.elf_name())
        .map(|section| DwarfSectionData::new(sect, section.data, section.header.sh_offset))
}

/// Reads a single `DwarfSection` from Mach object file.
fn read_mach_dwarf_section<'data>(
    macho: &mach::MachO<'data>,
    sect: DwarfSection,
) -> Option<DwarfSectionData<'data>> {
    find_mach_section(macho, sect.macho_name())
        .map(|section| DwarfSectionData::new(sect, section.data, section.header.offset.into()))
}
