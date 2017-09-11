#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub enum DwarfSection {
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

#[derive(Eq, PartialEq, Debug, Clone)]
pub struct DwarfSectionData<'a> {
    section: DwarfSection,
    data: &'a [u8],
    offset: usize,
}
